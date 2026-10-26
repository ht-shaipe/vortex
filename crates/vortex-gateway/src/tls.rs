//! 本地 HTTPS 证书管理。
//!
//! 职责：
//! 1. 外部证书优先加载（`VORTEX_TLS_CERT` / `VORTEX_TLS_KEY`）；
//! 2. 未提供外部证书时，首次启用自动生成本地 CA（10 年）+ 服务器证书
//!    （SAN: localhost / 127.0.0.1 / ::1 / 本机主机名(.local) / 本机私网 IP），持久化到
//!    `<data_dir>/tls/`，复用 `.encryption_key` 的安全落盘模式（0600）；
//! 3. macOS 上 best-effort 将 CA 装入用户登录钥匙串（无需 sudo），
//!    使 WKWebView / Chrome / curl 直接信任 `https://localhost:<port>`；
//! 4. HTTPS 握手下发完整证书链（叶子 + CA），自签根受信任的客户端均可直接校验；
//! 5. SAN 集合变化（如局域网 IP 变动）时透明重签叶子证书，CA 保持不变（已信任的根不受影响）。

use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use chrono::Datelike;

use log::{info, warn};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};

/// TLS 证书文件集合（位于 `<data_dir>/tls/` 下）。
pub struct TlsFiles {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    /// 服务器证书 SAN 集合记录（缺省名 `sans.txt`），签名集变化即触发重签叶子证书。
    pub sans_record: PathBuf,
    /// CA 版本标记（缺省名 `ca_meta.txt`）；旧版生成的 CA 不写此文件。
    /// 存在即代表 CA 由现行代码生成（有效期合规，能通过 Apple 信任策略评估）。
    pub ca_meta: PathBuf,
}

/// CA 版本标记内容：现行生成代码已产出"Apple 信任策略合规"的 CA。
const CA_META_VERSION: &str = "v2\n";

/// 确保 TLS 材料就绪并构建 rustls 服务端配置。
///
/// - 外部证书（`config.tls_cert` / `config.tls_key`）优先；
/// - 否则使用/生成 `<data_dir>/tls/` 下的自签证书；
/// - 非外部证书路径时尝试把 CA 装入 macOS 用户钥匙串。
///
/// - `config`：应用配置
/// - 返回值：可用于 `bind_rustls` 的服务端 TLS 配置
pub fn ensure_tls(config: &vortex_store::config::AppConfig) -> Result<rustls::ServerConfig, String> {
    // 外部证书模式：只加载，不做任何生成与信任安装
    if let (Some(cert), Some(key)) = (config.tls_cert.as_deref(), config.tls_key.as_deref()) {
        info!("[TLS] 使用外部证书: {} / {}", cert, key);
        let cert_pem = std::fs::read_to_string(cert).map_err(|e| format!("读取证书失败: {}", e))?;
        let key_pem = std::fs::read_to_string(key).map_err(|e| format!("读取私钥失败: {}", e))?;
        return build_server_config(&cert_pem, &key_pem);
    }

    // 自签证书模式：CA 常驻（不轻易更换，信任关系长期有效），叶子证书按需重签
    let dir = config.data_dir.join("tls");
    let files = TlsFiles {
        ca_cert: dir.join("ca.pem"),
        ca_key: dir.join("ca.key"),
        server_cert: dir.join("server.pem"),
        server_key: dir.join("server.key"),
        sans_record: dir.join("sans.txt"),
        ca_meta: dir.join("ca_meta.txt"),
    };

    if !files.ca_cert.exists() || !files.ca_key.exists() {
        info!("[TLS] 生成自签证书到 {}", dir.display());
        generate_certificates(&dir, &files)?;
        write_secure(&files.sans_record, &sans_record(), false)?;
    } else if !files.ca_meta.exists() {
        // 旧版升级：ca_meta 标记缺失说明证书由早期代码生成。实测（security
        // verify-cert -p ssl）锚（CA）的生效期不受 Apple 日期策略约束
        // （notBefore 1975 的 CA 依旧被接受为锚，仅当叶子生效期合法时通过），
        // 故保留 CA（信任关系与指纹均不变），只做一次性清理与标记：
        // - 清除登录钥匙串中同 CN 的历史残留 CA（多把不同密钥的同名证书
        //   会干扰钥匙串信任评估的锚选择）
        // - 补写 meta；叶子重签由下方记录校验触发（记录含规格版本行）
        info!("[TLS] 检测到旧版自签证书（叶子生效期不符合 Apple 信任策略），刷新服务器证书并清理钥匙串重复 CA");
        if is_default_data_dir(&dir) {
            purge_installed_cas();
        }
        write_secure(&files.ca_meta, CA_META_VERSION, false)?;
    } else {
        info!("[TLS] 复用已有自签 CA: {}", dir.display());
    }

    // 叶子证书：缺失或记录变化（SAN 集合变动 / 证书规格版本更新）时重签；CA 不变，已信任的根不受影响
    let record = std::fs::read_to_string(&files.sans_record).unwrap_or_default();
    if record.trim() != sans_record() {
        let reason = if record.is_empty() { "记录缺失" } else { "SAN 或规格变化" };
        info!("[TLS] 重新签发服务器证书（{}）", reason);
        generate_leaf_signed_by_ca(&files)?;
        write_secure(&files.sans_record, &sans_record(), false)?;
    } else {
        info!("[TLS] 复用已有服务器证书: {}", dir.display());
    }

    // 信任安装在后台线程执行：add-trusted-cert 可能等待 GUI 授权确认，
    // 阻塞主路径会导致端口无法绑定
    spawn_ca_trust_install(files.ca_cert.clone());

    // 握手下发完整证书链（叶子在前，CA 居后），避免客户端无法自行构建证书链
    let cert_pem = std::fs::read_to_string(&files.server_cert).map_err(|e| format!("读取证书失败: {}", e))?;
    let ca_pem = std::fs::read_to_string(&files.ca_cert).map_err(|e| format!("读取 CA 失败: {}", e))?;
    let key_pem = std::fs::read_to_string(&files.server_key).map_err(|e| format!("读取私钥失败: {}", e))?;
    let chain = format!("{}\n{}", cert_pem.trim_end(), ca_pem.trim_end());
    build_server_config(&chain, &key_pem)
}

/// 生成 CA 并落盘（叶子证书由调用方按需签发）。
///
/// CA 有效期 10 年；CN 为 `Vortex Local CA`。
///
/// - `dir`：目标目录（自动创建）
/// - `files`：证书文件集合
fn generate_certificates(dir: &Path, files: &TlsFiles) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建 TLS 目录失败: {}", e))?;

    // ---- 本地 CA（from_params 内部生成密钥，serialize_pem 自签） ----
    let mut ca_params = ca_params();
    // CA 有效期 10 年（自 generated_at - 1 天起，容忍轻微时钟偏移）
    let (y0, m0, d0) = today_ymd(-1);
    ca_params.not_before = date_time_ymd(y0, m0, d0);
    let (y, m, d) = today_ymd(3650);
    ca_params.not_after = date_time_ymd(y, m, d);
    let ca = Certificate::from_params(ca_params).map_err(|e| format!("生成 CA 证书失败: {}", e))?;
    let ca_pem = ca
        .serialize_pem()
        .map_err(|e| format!("序列化 CA 证书失败: {}", e))?;
    let ca_key_pem = ca.get_key_pair().serialize_pem();

    write_secure(&files.ca_cert, &ca_pem, false)?;
    write_secure(&files.ca_key, &ca_key_pem, true)?;
    write_secure(&files.ca_meta, CA_META_VERSION, false)?;

    // 紧接着用刚生成的 CA 签发叶子证书并写盘
    generate_leaf_signed_by_ca(files)
}

/// 用磁盘上已有的 CA 给服务器证书签发/重签并落盘。
///
/// CA 公私钥从 `ca.key` 加载后重建签发者；叶子 SAN 取当前机器实况（回环 + 主机名 + 私网 IP）。
///
/// - `files`：证书文件集合（`ca.key` 必须在场）
fn generate_leaf_signed_by_ca(files: &TlsFiles) -> Result<(), String> {
    let ca_key_pem = std::fs::read_to_string(&files.ca_key).map_err(|e| format!("读取 CA 私钥失败: {}", e))?;
    let ca_key = KeyPair::from_pem(&ca_key_pem).map_err(|e| format!("解析 CA 私钥失败: {}", e))?;
    // 以 CA 私钥重建签发者（subject/key 与磁盘上的 ca.pem 一致，信任锚不受重签影响）
    let mut issuer_params = ca_params();
    issuer_params.key_pair = Some(ca_key);
    let ca = Certificate::from_params(issuer_params).map_err(|e| format!("重建 CA 签发者失败: {}", e))?;

    let mut srv_params = CertificateParams::default();
    srv_params.distinguished_name.push(DnType::CommonName, "localhost");
    srv_params.subject_alt_names = sans_from_spec(&sans_spec());
    srv_params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    srv_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    // not_before 用签发瞬间。Apple 信任策略会拒绝"早于签发时刻约 1~2 小时以上"
    // 的叶子证书（实测 -1h 过、-2h 拒，评估器报 CERT_SUSPENDED，WKWebView/URLSession
    // 即报 -1202 不可信；curl/openssl 自行构链该策略不可见）。锚（CA）的生效期不受此约束。
    srv_params.not_before = time::OffsetDateTime::now_utc();
    let (y, m, d) = today_ymd(825);
    srv_params.not_after = date_time_ymd(y, m, d);
    let server = Certificate::from_params(srv_params)
        .map_err(|e| format!("生成服务器证书失败: {}", e))?;
    let server_pem = server
        .serialize_pem_with_signer(&ca)
        .map_err(|e| format!("CA 签发服务器证书失败: {}", e))?;
    let server_key_pem = server.get_key_pair().serialize_pem();

    write_secure(&files.server_cert, &server_pem, false)?;
    write_secure(&files.server_key, &server_key_pem, true)?;
    info!("[TLS] 服务器证书 SAN: {}", sans_record());
    info!(
        "[TLS] 提示: 客户端请信任 CA（ca.pem；如 NODE_EXTRA_CA_CERTS=<ca.pem> 路径）而非叶子证书（server.pem），重签后叶子会更换"
    );
    Ok(())
}

/// CA 证书参数（CN: Vortex Local CA，可签发子证书与 CRL；有效期由调用方设置）。
fn ca_params() -> CertificateParams {
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Vortex Local CA");
    ca_params
}

/// 以今天（UTC）为基准偏移 `days` 天后的年月日（负数向前）。
fn today_ymd(days: i64) -> (i32, u8, u8) {
    let d = chrono::Utc::now() + chrono::Duration::days(days);
    (d.year(), d.month() as u8, d.day() as u8)
}

/// SAN 规格：`(kind, value)` 列表，kind 为 `dns` / `ip`。
///
/// 覆盖 localhost、回环地址、本机主机名与本机私网 IP（局域网 / Tailscale 等访问入口）。
type SansSpec = Vec<(&'static str, String)>;

/// 当前机器的 SAN 规格。
fn sans_spec() -> SansSpec {
    let mut spec: SansSpec = vec![
        ("dns", "localhost".to_string()),
        ("ip", "127.0.0.1".to_string()),
        ("ip", "::1".to_string()),
    ];
    // 追加本机主机名（失败不影响）
    if let Ok(name) = hostname() {
        if !name.is_empty() && name != "localhost" {
            spec.push(("dns", name.clone()));
            // mDNS/Bonjour 名（局域网设备常用 `主机名.local` 访问）
            spec.push(("dns", format!("{name}.local")));
        }
    }
    // 追加本机私网 IP（按地址排序去重）
    let mut ips = local_private_ips();
    ips.sort();
    ips.dedup();
    for ip in ips {
        spec.push(("ip", ip.to_string()));
    }
    spec
}

/// SAN 集合的持久化文本（`sans.txt` 内容，排序去掉来源顺序影响）。
///
/// 尾部带一条固定规格版本行：调整叶子证书签发参数（如 notBefore 取签发瞬间）
/// 时递增此标记，使所有已有安装在下一次启动时因记录不匹配而自动重签一次。
fn sans_record() -> String {
    let mut lines: Vec<String> = sans_spec()
        .into_iter()
        .map(|(kind, value)| format!("{}:{}", kind, value))
        .collect();
    // v2: 叶子 notBefore 由"回拨 3 天"改为签发瞬间（Apple 信任策略拒绝回拨叶子）
    lines.push("spec:v2/nb-issue-time".to_string());
    lines.sort();
    lines.join("\n")
}

/// 把 SAN 规格转成 rcgen 的 SanType 集合。
fn sans_from_spec(spec: &SansSpec) -> Vec<SanType> {
    spec.iter()
        .map(|(kind, value)| match *kind {
            "dns" => SanType::DnsName(value.clone()),
            // 全部 ip 记录都来自合法地址文本或常量，解析必成功
            _ => SanType::IpAddress(value.parse().expect("san ip")),
        })
        .collect()
}

/// 本机非回环 IPv4 地址（仅私网 10/172.16-31/192.168 与 CGNAT 100.64/10 段，上限 8 个）。
fn local_private_ips() -> Vec<IpAddr> {
    let mut ips = parse_inet_texts(&inet_lines());
    // 只保留常用私网段，避免把公网出口 IP 写进证书
    ips.retain(|ip| is_private(*ip));
    ips.truncate(8);
    ips
}

/// 各平台的本机 inet 地址文本（IP 一行一个候选，解析交给调用方过滤）。
fn inet_lines() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        // macOS: ifconfig 输出中的 "inet x.x.x.x ..." 行
        Command::new("ifconfig")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).collect())
            .unwrap_or_default()
    }
    #[cfg(target_os = "linux")]
    {
        // Linux: hostname -I 直接给出空格分隔的地址列表
        Command::new("hostname")
            .arg("-I")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// 从文本行中提取 IPv4 地址（macOS 的 "inet 1.2.3.4 netmask ..." 行）。
fn parse_inet_texts(lines: &[String]) -> Vec<IpAddr> {
    let mut ips = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("inet ") {
            if let Ok(ip) = rest.split_whitespace().next().unwrap_or("").parse::<IpAddr>() {
                ips.push(ip);
            }
        } else if let Ok(ip) = line.trim().parse::<IpAddr>() {
            // Linux hostname -I: 整行是一个地址
            ips.push(ip);
        }
    }
    ips
}

/// 是否私网地址（不含回环；100.64.0.0/10 覆盖 Tailscale 等 CGNAT）。
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => match v.octets() {
            [10, ..] | [192, 168, ..] => true,
            [172, b, ..] => (16..=31).contains(&b),
            [100, b, ..] => (64..=127).contains(&b),
            _ => false,
        },
        IpAddr::V6(_) => false,
    }
}

/// 读取本机主机名（去除局域网后缀），失败返回 Err。
fn hostname() -> Result<String, ()> {
    let out = Command::new("hostname").output().map_err(|_| ())?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
    let s = s.split('.').next().unwrap_or("").to_string();
    if s.is_empty() { Err(()) } else { Ok(s) }
}

/// 写文件并按需收紧权限（私钥 0600），模式参考 `.encryption_key` 的落盘逻辑。
fn write_secure(path: &Path, content: &str, is_private: bool) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("写入 {} 失败: {}", path.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if is_private { 0o600 } else { 0o644 };
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    Ok(())
}

/// 在独立线程中执行 CA 信任安装（不阻塞服务启动）。
///
/// `security add-trusted-cert` 可能弹出 GUI 授权确认或等待较久，
/// 放入后台线程执行，完成与否均只记日志。
fn spawn_ca_trust_install(ca_path: PathBuf) {
    std::thread::spawn(move || {
        // 仅默认数据目录时安装信任：测试/临时目录的 CA 不应写入全局钥匙串
        // （传入的是 CA 文件路径，沿祖先链匹配数据目录）
        if !is_default_data_dir(&ca_path) {
            return;
        }
        try_install_ca_trust(&ca_path);
    });
}

/// 是否位于默认用户数据目录（`~/Library/Application Support/vortex`）之下。
///
/// 钥匙串安装/清理等全局副作用只允许发生在默认目录；测试或自定义目录时跳过。
/// 按祖先链匹配：传入数据目录本身、其 `tls` 子目录或目录内文件均可判定。
fn is_default_data_dir(dir: &Path) -> bool {
    dir.ancestors()
        .any(|p| p.to_string_lossy().ends_with("Library/Application Support/vortex"))
}

/// 清除钥匙串中全部同名 `Vortex Local CA`（历史版本残留的多张旧 CA）。
///
/// 同一 CN 存在多把不同密钥的 CA 时，SecTrust 按 subject 匹配信任锚可能选中
/// 被 Apple 策略拒绝的旧款（如万年有效期证书），导致整链被判 CERT_SUSPENDED。
/// 清空后由后续流程安装当前磁盘上唯一合规的 CA。
#[cfg(target_os = "macos")]
fn purge_installed_cas() {
    let home = std::env::var("HOME").unwrap_or_default();
    let keychain = format!("{}/Library/Keychains/login.keychain-db", home);
    let mut removed = 0usize;
    for _ in 0..64 {
        let out = Command::new("security")
            .args(["delete-certificate", "-c", "Vortex Local CA"])
            .arg(&keychain)
            .output();
        match out {
            Ok(o) if o.status.success() => removed += 1,
            Ok(o) => {
                if removed == 0 {
                    warn!(
                        "[TLS] 钥匙串清理未能开始（{}）",
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
                break;
            }
            Err(e) => {
                warn!("[TLS] 无法调用 security 清理钥匙串: {}", e);
                break;
            }
        }
    }
    if removed > 0 {
        info!("[TLS] 已从登录钥匙串清除 {} 条旧 CA 记录", removed);
    }
}

#[cfg(not(target_os = "macos"))]
fn purge_installed_cas() {}

/// macOS 上把 CA 装入用户登录钥匙串（best-effort，无需 sudo）。
///
/// 已信任或非 macOS 平台时静默跳过；失败时日志给出手动导入指引。
fn try_install_ca_trust(ca_path: &Path) {
    if !cfg!(target_os = "macos") {
        return;
    }
    // 已在信任列表中则跳过（用 SSL 策略 + 主机名评估，与 WKWebView/Safari 的
    // 信任评估对齐；基础策略会漏判被 Apple 信任策略拒绝的证书）
    let trusted = Command::new("security")
        .args(["verify-cert", "-c", &server_path_of(ca_path), "-p", "ssl", "-n", "localhost"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if trusted {
        info!("[TLS] CA 已受信任，跳过安装");
        return;
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let keychain = format!("{}/Library/Keychains/login.keychain-db", home);
    let out = Command::new("security")
        .args(["add-trusted-cert", "-r", "trustRoot", "-k", &keychain])
        .arg(ca_path)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            info!("[TLS] CA 已装入登录钥匙串（用户信任域），https://localhost 将不再告警");
        }
        Ok(o) => {
            warn!(
                "[TLS] CA 自动装入钥匙串失败（{}），请手动信任: 打开「钥匙串访问」导入 {} 并设为始终信任",
                String::from_utf8_lossy(&o.stderr).trim(),
                ca_path.display()
            );
        }
        Err(e) => warn!("[TLS] 无法调用 security 命令: {}（请手动信任 CA）", e),
    }
}

/// 由 CA 证书路径推出服务器证书路径（verify-cert 用）。
fn server_path_of(ca_path: &Path) -> String {
    ca_path
        .parent()
        .map(|p| p.join("server.pem").display().to_string())
        .unwrap_or_default()
}

/// 从 PEM 文本组装 rustls 服务端配置。
fn build_server_config(cert_pem: &str, key_pem: &str) -> Result<rustls::ServerConfig, String> {
    let certs: Vec<rustls::Certificate> = rustls_pemfile::certs(&mut cert_pem.as_bytes())
        .map_err(|e| format!("解析证书失败: {}", e))?
        .into_iter()
        .map(rustls::Certificate)
        .collect();
    if certs.is_empty() {
        return Err("证书文件中未找到有效证书".to_string());
    }
    let key = load_private_key(key_pem)?;
    rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("加载 TLS 配置失败: {}", e))
}

/// 解析 PEM 私钥，依次尝试 PKCS8 / RSA / EC 格式。
fn load_private_key(pem: &str) -> Result<rustls::PrivateKey, String> {
    for parser in [
        rustls_pemfile::pkcs8_private_keys,
        rustls_pemfile::rsa_private_keys,
        rustls_pemfile::ec_private_keys,
    ] {
        if let Ok(keys) = parser(&mut pem.as_bytes()) {
            if let Some(k) = keys.into_iter().next() {
                return Ok(rustls::PrivateKey(k));
            }
        }
    }
    Err("私钥文件中未找到可用密钥（支持 PKCS8/RSA/EC）".to_string())
}

//! 本地 HTTPS 证书管理。
//!
//! 职责：
//! 1. 外部证书优先加载（`VORTEX_TLS_CERT` / `VORTEX_TLS_KEY`）；
//! 2. 未提供外部证书时，首次启用自动生成本地 CA（10 年）+ 服务器证书
//!    （SAN: localhost / 127.0.0.1 / ::1 / 本机主机名），持久化到
//!    `<data_dir>/tls/`，复用 `.encryption_key` 的安全落盘模式（0600）；
//! 3. macOS 上 best-effort 将 CA 装入用户登录钥匙串（无需 sudo），
//!    使 WKWebView / Chrome / curl 直接信任 `https://localhost:<port>`。

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use log::{info, warn};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyUsagePurpose, SanType,
};

/// TLS 证书文件集合（位于 `<data_dir>/tls/` 下）。
pub struct TlsFiles {
    pub ca_cert: PathBuf,
    pub ca_key: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
}

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

    // 自签证书模式：检查 → 缺失则生成 → 尝试装信任
    let dir = config.data_dir.join("tls");
    let files = TlsFiles {
        ca_cert: dir.join("ca.pem"),
        ca_key: dir.join("ca.key"),
        server_cert: dir.join("server.pem"),
        server_key: dir.join("server.key"),
    };

    let all_exist = [(&files.ca_cert, "ca.pem"), (&files.ca_key, "ca.key"), (&files.server_cert, "server.pem"), (&files.server_key, "server.key")]
        .iter()
        .all(|(p, _)| p.exists());
    if !all_exist {
        info!("[TLS] 生成自签证书到 {}", dir.display());
        generate_certificates(&dir, &files)?;
    } else {
        info!("[TLS] 复用已有自签证书: {}", dir.display());
    }

    // 信任安装在后台线程执行：add-trusted-cert 可能等待 GUI 授权确认，
    // 阻塞主路径会导致端口无法绑定
    spawn_ca_trust_install(files.ca_cert.clone());

    let cert_pem = std::fs::read_to_string(&files.server_cert).map_err(|e| format!("读取证书失败: {}", e))?;
    let key_pem = std::fs::read_to_string(&files.server_key).map_err(|e| format!("读取私钥失败: {}", e))?;
    build_server_config(&cert_pem, &key_pem)
}

/// 生成 CA 与服务器证书并落盘。
///
/// CA 有效期 10 年；服务器证书有效期 825 天（跨平台工具链兼容的最大常用值），
/// SAN 覆盖 localhost、127.0.0.1、::1 与本机主机名。
///
/// - `dir`：目标目录（自动创建）
/// - `files`：四个文件的输出路径
fn generate_certificates(dir: &Path, files: &TlsFiles) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建 TLS 目录失败: {}", e))?;

    // ---- 本地 CA（from_params 内部生成密钥，serialize_pem 自签） ----
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Vortex Local CA");
    let ca = Certificate::from_params(ca_params).map_err(|e| format!("生成 CA 证书失败: {}", e))?;

    // ---- 服务器证书（由 CA 签发） ----
    let mut srv_params = CertificateParams::default();
    srv_params.distinguished_name.push(DnType::CommonName, "localhost");
    srv_params.subject_alt_names = server_sans();
    srv_params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    srv_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server = Certificate::from_params(srv_params)
        .map_err(|e| format!("生成服务器证书失败: {}", e))?;

    let server_pem = server
        .serialize_pem_with_signer(&ca)
        .map_err(|e| format!("CA 签发服务器证书失败: {}", e))?;
    let server_key_pem = server.get_key_pair().serialize_pem();
    let ca_pem = ca
        .serialize_pem()
        .map_err(|e| format!("序列化 CA 证书失败: {}", e))?;
    let ca_key_pem = ca.get_key_pair().serialize_pem();

    write_secure(&files.server_cert, &server_pem, false)?;
    write_secure(&files.server_key, &server_key_pem, true)?;
    write_secure(&files.ca_cert, &ca_pem, false)?;
    write_secure(&files.ca_key, &ca_key_pem, true)?;
    Ok(())
}

/// 服务器证书的 SAN 列表：localhost + 回环地址 + 本机主机名。
fn server_sans() -> Vec<SanType> {
    let mut sans = vec![
        SanType::DnsName("localhost".to_string()),
        SanType::IpAddress(IpAddr::from([127, 0, 0, 1])),
        SanType::IpAddress(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1])),
    ];
    // 追加本机主机名（失败不影响）
    if let Ok(name) = hostname() {
        if !name.is_empty() && name != "localhost" {
            sans.push(SanType::DnsName(name));
        }
    }
    sans
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
        try_install_ca_trust(&ca_path);
    });
}

/// macOS 上把 CA 装入用户登录钥匙串（best-effort，无需 sudo）。
///
/// 已信任或非 macOS 平台时静默跳过；失败时日志给出手动导入指引。
fn try_install_ca_trust(ca_path: &Path) {
    if !cfg!(target_os = "macos") {
        return;
    }
    // 已在信任列表中则跳过（verify-cert 校验服务器证书链是否被信任）
    let trusted = Command::new("security")
        .args(["verify-cert", "-c", &server_path_of(ca_path)])
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

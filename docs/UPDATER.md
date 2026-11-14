# Vortex 自动更新

Vortex 使用 [Tauri 2 Updater 插件](https://v2.tauri.app/plugin/updater/) 实现自动更新，更新源托管在 GitHub Releases。

## 工作原理

```
应用启动 → 自动检查 GitHub Releases → 发现新版本 → 提示用户 → 下载 + 签名验证 → 安装 + 重启
     ↑
     └── 运行期间每 6 小时自动复查一次（同一版本只提醒一次）
```

1. 应用启动时自动调用 `check()` 检查更新
2. 应用持续运行期间每 6 小时周期性复查（`useUpdater.ts` 的 `startPeriodicCheck`，正在检查/下载/待安装时跳过本轮）
3. 发现新版本后弹出通知（同一版本仅提醒一次），用户可前往「关于」页查看详情
4. 用户点击「下载并安装」后，下载更新包并验证 Ed25519 签名
5. 验证通过后安装更新并自动重启应用

## 更新源

当前更新源配置在 `tauri.conf.json` 中（hub 优先、GitHub 兜底，依次尝试）：

```json
"endpoints": [
  "https://hub.htui.cc/apps/vortex/latest.json",
  "https://github.com/ht-shaipe/vortex/releases/latest/download/latest.json"
]
```

`latest.json` 由 GitHub Actions 在发布时自动生成：清单内的下载地址全部指向 hub 扁平路径
（`vortex_windows.exe` / `vortex_macos_{arch}.app.tar.gz` / `vortex_linux.AppImage`），
对应产物由 `hub-upload` 任务上传到 hub，国内下载走 hub、海外走 GitHub 双保险。

## 首次配置

### 1. 生成签名密钥

```bash
bash scripts/generate-updater-key.sh
```

这会生成两个文件：
- `~/.vortex-updater.key` — 私钥（用于签名更新包，**不要提交到仓库**）
- `~/.vortex-updater.key.pub` — 公钥（用于验证更新包）

### 2. 配置公钥

将公钥内容复制到 `src-tauri/tauri.conf.json`：

```json
"plugins": {
  "updater": {
    "pubkey": "你的公钥内容"
  }
}
```

### 3. 配置 GitHub Secrets

在 [GitHub 仓库设置页面](https://github.com/ht-shaipe/vortex/settings/secrets/actions) 添加以下 Secrets：

| Secret 名 | 值 | 说明 |
|-----------|-----|------|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥文件内容 | 用于签名更新包 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 | 如果私钥有密码 |
| `HUB_UPLOAD_TOKEN` | hub 发版上传令牌 | 推送产物到 hub.htui.cc（`X-Upload-Token` 请求头） |

## 发布新版本

### 方式一：打 Tag 自动发布

```bash
git tag v0.2.0
git push origin v0.2.0
```

GitHub Actions 会自动：
1. 在 macOS (arm64 + x64)、Linux、Windows 上构建
2. 签名更新包
3. 创建 GitHub Release
4. 上传 `latest.json` 和各平台安装包
5. 推送 `latest.json` + 各平台安装包到 hub.htui.cc 发版接口（`hub-upload` job，appKey 为 `vortex`，可在 hub 后台下载/展示最新版本）

### 方式二：手动触发

在 [GitHub Actions 页面](https://github.com/ht-shaipe/vortex/actions/workflows/release.yml) 点击 "Run workflow"，输入版本号即可。

## 切换更新源

当 hub/GitHub 访问困难时，可调整 `endpoints` 顺序或增删源，Tauri 支持多个 endpoints 并依次尝试：

```json
"endpoints": [
  "https://cdn.example.com/vortex/latest.json",
  "https://hub.htui.cc/apps/vortex/latest.json"
]
```

## 文件结构

```
src/composables/useUpdater.ts     # 更新检查 composable（启动检查 + 周期复查）
src/views/About.vue               # 「关于」页检查更新 UI
src-tauri/tauri.conf.json         # updater 插件配置
.github/workflows/release.yml     # 自动发布 workflow
.github/workflows/hub-upload.yml  # 可复用工作流：发版产物推送 hub.htui.cc
scripts/generate-updater-key.sh   # 密钥生成脚本
```

## 安装模式

当前配置为 `passive` 模式（Windows）：
- 下载完成后提示用户确认安装
- 安装完成后自动重启

可选模式：
- `passive` — 提示后安装（推荐）
- `basicUi` — 显示基本安装界面
- `quiet` — 静默安装

# Vortex 自动更新

Vortex 使用 [Tauri 2 Updater 插件](https://v2.tauri.app/plugin/updater/) 实现自动更新，更新源托管在 GitHub Releases。

## 工作原理

```
应用启动 → 自动检查 GitHub Releases → 发现新版本 → 提示用户 → 下载 + 签名验证 → 安装 + 重启
```

1. 应用启动时自动调用 `check()` 检查更新
2. 发现新版本后弹出通知，用户可前往「检查更新」页面查看详情
3. 用户点击「下载并安装」后，下载更新包并验证 Ed25519 签名
4. 验证通过后安装更新并自动重启应用

## 更新源

当前更新源配置在 `tauri.conf.json` 中：

```json
"endpoints": [
  "https://github.com/th-shaipe/vortex/releases/latest/download/latest.json"
]
```

`latest.json` 由 GitHub Actions 在发布时自动生成并上传到 Release assets。

> **后续可切换到国内镜像**：只需将 `endpoints` 改为国内 CDN/Gitee Release 地址即可，无需修改代码。

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

在 [GitHub 仓库设置页面](https://github.com/th-shaipe/vortex/settings/secrets/actions) 添加以下 Secrets：

| Secret 名 | 值 | 说明 |
|-----------|-----|------|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥文件内容 | 用于签名更新包 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 | 如果私钥有密码 |

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

### 方式二：手动触发

在 [GitHub Actions 页面](https://github.com/th-shaipe/vortex/actions/workflows/release.yml) 点击 "Run workflow"，输入版本号即可。

## 切换到国内更新源

当 GitHub 访问困难时，可将更新源切换到国内镜像：

### 方案 A：Gitee 镜像

1. 在 Gitee 创建同名仓库并同步 Release
2. 修改 `tauri.conf.json`：

```json
"endpoints": [
  "https://gitee.com/th-shaipe/vortex/releases/latest/download/latest.json"
]
```

### 方案 B：自建 CDN

1. 将 `latest.json` 和更新包上传到 CDN
2. 修改 `endpoints` 为 CDN 地址

### 方案 C：多源回退

Tauri 支持多个 endpoints，会依次尝试：

```json
"endpoints": [
  "https://cdn.example.com/vortex/latest.json",
  "https://github.com/th-shaipe/vortex/releases/latest/download/latest.json"
]
```

## 文件结构

```
src/composables/useUpdater.ts     # 更新检查 composable
src/views/Updates.vue             # 更新页面 UI
src-tauri/tauri.conf.json         # updater 插件配置
.github/workflows/release.yml     # 自动发布 workflow
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

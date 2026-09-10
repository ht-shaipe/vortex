#!/bin/bash
# 生成 Tauri Updater 签名密钥对
# 运行方式: bash scripts/generate-updater-key.sh

set -e

KEY_PATH="${HOME}/.vortex-updater.key"
PUB_PATH="${KEY_PATH}.pub"

echo "=== Vortex Updater 签名密钥生成器 ==="
echo ""

# 检查是否已存在
if [ -f "$KEY_PATH" ]; then
  echo "警告: 私钥已存在于 $KEY_PATH"
  read -p "是否覆盖? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "已取消"
    exit 0
  fi
fi

# 使用 tauri CLI 生成密钥
if command -v cargo-tauri &> /dev/null; then
  cargo-tauri signer generate -w "$KEY_PATH"
elif [ -f "node_modules/.bin/tauri" ]; then
  node_modules/.bin/tauri signer generate -w "$KEY_PATH"
elif command -v npx &> /dev/null; then
  npx @tauri-apps/cli signer generate -w "$KEY_PATH"
else
  echo "错误: 未找到 tauri CLI，请先安装: cargo install tauri-cli --version \"^2\""
  exit 1
fi

echo ""
echo "=== 密钥生成完成 ==="
echo "私钥: $KEY_PATH"
echo "公钥: $PUB_PATH"
echo ""
echo "下一步:"
echo "1. 将公钥内容复制到 src-tauri/tauri.conf.json 的 plugins.updater.pubkey 字段"
echo "2. 将私钥内容设置为 GitHub Secret: TAURI_SIGNING_PRIVATE_KEY"
echo "3. 如果私钥有密码，设置为 GitHub Secret: TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
echo ""
echo "GitHub Secret 设置页面: https://github.com/th-shaipe/vortex/settings/secrets/actions"

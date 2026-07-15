#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for command_name in npm cargo; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "缺少 $command_name。请先安装 Node.js 22 和 stable Rust，随后重新运行 ./start.sh。" >&2
    exit 1
  fi
done

mkdir -p data

echo "[1/3] 安装前端依赖"
npm --prefix web ci
echo "[2/3] 构建谜局AI Web 界面"
npm --prefix web run build
echo "[3/3] 启动谜局AI"
echo "打开 http://127.0.0.1:${NARRASTATE_PORT:-3000}"
echo "首次使用：进入“设置”，填写 OpenAI-compatible Base URL、模型和 API Key，然后测试并保存。"
echo "API Key 会保存在本机 data/provider.env；未配置时仍可使用 Mock 模式。"
exec cargo run --locked -p narrastate-server -- serve \
  --db "${DATABASE_URL:-data/narrastate.db}" \
  --cases cases \
  --web web/dist

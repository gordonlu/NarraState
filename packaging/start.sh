#!/usr/bin/env bash
set -euo pipefail

app_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$app_root"
mkdir -p data

echo "谜局AI 正在启动：http://127.0.0.1:${NARRASTATE_PORT:-3000}"
echo "首次使用请在网页“设置”中填写模型服务，不要把 API Key 写入此脚本。"
exec ./narrastate-server serve \
  --db "${DATABASE_URL:-data/narrastate.db}" \
  --cases cases \
  --web web

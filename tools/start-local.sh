#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

npm --prefix web ci
npm --prefix web run build
exec cargo run --locked -p narrastate-server -- serve \
  --db "${DATABASE_URL:-data/narrastate.db}" \
  --cases cases \
  --web web/dist

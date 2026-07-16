#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

sensitive_files="$(git ls-files | grep -E '(^|/)(\.env|provider\.env|image-provider\.env|id_rsa|id_ed25519|[^/]+\.pem)$' | grep -vE '(^|/)\.env\.example$' || true)"
if [[ -n "$sensitive_files" ]]; then
  echo "Tracked sensitive files are forbidden:" >&2
  echo "$sensitive_files" >&2
  exit 1
fi

if matches="$(git grep -nE '(sk-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----)' -- . ':!tools/check-tracked-secrets.sh' || true)" && [[ -n "$matches" ]]; then
  echo "Possible secret material found in tracked content:" >&2
  echo "$matches" >&2
  exit 1
fi

echo "Tracked secret check passed."

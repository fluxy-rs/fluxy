#!/usr/bin/env bash
# M1-09 end-to-end acceptance wrapper for Linux and macOS.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if ! command -v node >/dev/null 2>&1; then
  echo "node is required to run the M1-09 end-to-end driver" >&2
  exit 127
fi

exec node "$here/e2e.mjs" "$@"

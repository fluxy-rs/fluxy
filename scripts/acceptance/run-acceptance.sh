#!/usr/bin/env bash
# M2-09 acceptance wrapper for Linux and macOS.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if ! command -v node >/dev/null 2>&1; then
  echo "node is required to run the M2-09 acceptance driver" >&2
  exit 127
fi

exec node "$here/acceptance.mjs" "$@"

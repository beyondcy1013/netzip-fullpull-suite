#!/usr/bin/env bash
set -euo pipefail

crate_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
stock_root="$(CDPATH= cd -- "$crate_dir/../.." && pwd)"

find "$stock_root" \
  -path '*/target' -prune -o \
  -path '*/diagnostics' -prune -o \
  -name Cargo.toml -type f -print0 |
  xargs -0 grep -lE 'netzip-fullpull[[:space:]]*=' |
  sort |
  sed "s#^$stock_root/##"

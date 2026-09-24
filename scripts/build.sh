#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root/frontend"
npm ci
npm run lint
npm run format:check
npm run typecheck
npm test
npm run build
cd "$project_root"
cargo build --locked --package codex2api "$@"

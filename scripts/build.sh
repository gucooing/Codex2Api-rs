#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
for frontend_project in frontend frontend-user; do
cd "$project_root/$frontend_project"
npm ci
npm run lint
npm run format:check
npm run typecheck
npm test
npm run build
done
cd "$project_root"
cargo build --locked --package codex2api "$@"

#!/usr/bin/env bash
# 一条命令启动：构建前端 -> 启动 Rust/Axum 后端（内置静态托管 + SQLite）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

PORT="${PORT:-8080}"
BIND_ADDR="0.0.0.0:${PORT}"
export DATABASE_URL="${DATABASE_URL:-sqlite://${ROOT}/data/meet.db?mode=rwc}"
export FRONTEND_DIR="${ROOT}/frontend/dist"
export BIND_ADDR

# 1) 选择/安装 Rust 工具链
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  else
    echo ">> 未检测到 cargo，正在安装 Rust 工具链…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
fi

# 2) 构建前端
if [ ! -d frontend/node_modules ]; then
  echo ">> 安装前端依赖…"
  (cd frontend && npm install)
fi
echo ">> 构建 Svelte 前端…"
(cd frontend && npm run build)

# 3) 启动后端（自动建库、建表、写入演示数据）
mkdir -p "$ROOT/data"
echo ">> 启动后端服务：http://localhost:${PORT}"
echo "   工作台  http://localhost:${PORT}/"
echo "   公示屏  http://localhost:${PORT}/#/screen"
exec cargo run --manifest-path "$ROOT/backend/Cargo.toml" --release

#!/usr/bin/env bash
# =====================================================================
# 一条命令启动：小学运动会检录与成绩更正系统
#   - 自动准备 Rust 工具链（缺失时安装 rustup stable）
#   - 安装前端依赖并构建 Svelte 静态资源
#   - 编译并启动 Axum 后端（同时托管前端 + API + SQLite）
#
# 用法：
#   ./start.sh                 # 默认 http://localhost:8080
#   PORT=9000 ./start.sh       # 自定义端口
#   RELEASE=1 ./start.sh       # release 编译（默认 debug，编译更快）
#   ./start.sh --verify        # 启动前先运行端到端验证脚本
# =====================================================================
set -euo pipefail
cd "$(dirname "$0")"

PORT="${PORT:-8080}"
PROFILE_FLAG=""
[ "${RELEASE:-0}" = "1" ] && PROFILE_FLAG="--release"
BIN="./target/$( [ "${RELEASE:-0}" = 1 ] && echo release || echo debug )/sports-meet"

echo "──────────────────────────────────────────────"
echo " 🏅 小学运动会检录与成绩更正系统 一键启动"
echo "──────────────────────────────────────────────"

# 0) 可选：先跑端到端验证
if [ "${1:-}" = "--verify" ]; then
  ./verify.sh
fi

# 1) Rust 工具链
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "▶ 未检测到 Rust 工具链，正在通过 rustup 安装 stable…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
echo "✔ cargo: $(cargo --version)"

# 2) 前端依赖
if [ ! -d frontend/node_modules ]; then
  echo "▶ 安装前端依赖 (npm install)…"
  (cd frontend && npm install)
fi

# 3) 构建前端
echo "▶ 构建 Svelte 前端…"
(cd frontend && npm run build)

# 4) 编译后端
echo "▶ 编译 Rust 后端…"
cargo build $PROFILE_FLAG

# 5) 启动（SQLite 文件自动创建在 data/meet.db）
mkdir -p data
echo ""
echo "──────────────────────────────────────────────"
echo " 🚀 启动中…  请用浏览器打开:  http://localhost:${PORT}"
echo "    公示屏直达: http://localhost:${PORT}/ （“📺 公示屏”标签）"
echo "    按 Ctrl-C 停止服务（数据保留在 data/meet.db）"
echo "──────────────────────────────────────────────"
exec env DATABASE_URL="sqlite:data/meet.db?mode=rwc" STATIC_DIR="frontend/dist" PORT="$PORT" "$BIN"

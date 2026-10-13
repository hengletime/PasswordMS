#!/usr/bin/env bash
# PasswordMS 一键构建（Linux / macOS）
# 用法：scripts/build.sh [desktop|android|all]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${1:-desktop}"

log()  { printf '\033[1;34m[build]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[warn ]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[fatal]\033[0m %s\n' "$*" >&2; exit 1; }

require() {
  command -v "$1" >/dev/null 2>&1 || die "缺少命令：$1（$2）"
}

check_env() {
  require node "请安装 Node.js"
  require npm  "请安装 npm"
  require cargo "请安装 Rust 工具链（本机当前未安装，无法构建桌面/核心）"
}

build_frontend() {
  log "构建前端（Vite → dist/）"
  if [ -d node_modules ]; then
    npm run build
  else
    log "未发现 node_modules，先执行 npm install"
    npm install
    npm run build
  fi
}

build_desktop() {
  log "构建桌面端（Tauri，Windows 目标请在 Windows 上运行 scripts/build.ps1）"
  npm run tauri build
}

build_android() {
  log "构建 Android（APK）"
  bash "$ROOT/scripts/build-android.sh"
}

case "$TARGET" in
  desktop)
    check_env
    build_frontend
    build_desktop
    bash "$ROOT/scripts/check-size.sh" desktop || warn "体积核验未通过"
    ;;
  android)
    if ! command -v cargo >/dev/null 2>&1; then
      warn "未检测到 Rust 工具链；Android 构建为本机不可用。"
      warn "请在已安装 Rust + NDK 的环境运行：scripts/build-android.sh"
      exit 0
    fi
    build_frontend
    build_android
    ;;
  all)
    check_env
    build_frontend
    build_desktop
    if command -v cargo >/dev/null 2>&1 && command -v sdkmanager >/dev/null 2>&1; then
      build_android
    else
      warn "跳过 Android：缺少 Rust 工具链或 Android SDK/NDK。"
    fi
    bash "$ROOT/scripts/check-size.sh" all || warn "体积核验未通过"
    ;;
  *)
    die "未知目标：$TARGET（可选 desktop|android|all）"
    ;;
esac

log "完成。"

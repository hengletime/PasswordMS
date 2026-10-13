#!/usr/bin/env bash
# PasswordMS Android APK 构建与体积核验
# 前置：Rust（含 android 目标）+ Android SDK/NDK + Tauri CLI
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APK_LIMIT_MB=12

log()  { printf '\033[1;34m[android]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[warn ]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[fatal]\033[0m %s\n' "$*" >&2; exit 1; }

if ! command -v cargo >/dev/null 2>&1; then
  die "未检测到 Rust 工具链（cargo）。Android 构建需要 Rust + android target（rustup target add aarch64-linux-android）。"
fi

if [ -z "${ANDROID_HOME:-}" ] && [ -z "${NDK_HOME:-}" ]; then
  warn "未检测到 ANDROID_HOME / NDK_HOME 环境变量；若构建失败请先配置 Android SDK/NDK。"
fi

log "确保 Android 目标已安装"
rustup target add aarch64-linux-android armv7-linux-androideabi 2>/dev/null || warn "rustup target add 失败，请手动确认目标已安装"

log "构建前端"
if [ -d node_modules ]; then npm run build; else npm install && npm run build; fi

log "构建 APK（Tauri android build）"
npm run tauri android build

log "定位 APK 产物"
APK="$(find src-tauri/gen/android -name '*.apk' -type f 2>/dev/null | head -n 1 || true)"
if [ -z "$APK" ]; then
  die "未找到 APK 产物，请检查 Android 构建输出。"
fi

SIZE_BYTES="$(stat -c%s "$APK" 2>/dev/null || stat -f%z "$APK")"
SIZE_MB="$(awk "BEGIN { printf \"%.2f\", $SIZE_BYTES/1048576 }")"
log "APK：$APK（${SIZE_MB} MB）"

awk "BEGIN { exit !($SIZE_MB <= $APK_LIMIT_MB) }" || die "APK 体积 ${SIZE_MB} MB 超出上限 ${APK_LIMIT_MB} MB"
log "体积核验通过（≤ ${APK_LIMIT_MB} MB）。"

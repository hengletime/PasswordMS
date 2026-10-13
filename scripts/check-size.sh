#!/usr/bin/env bash
# PasswordMS 体积核验（CI 门禁）
# Windows 安装包 ≤ 10 MB；Android APK ≤ 12 MB
# 用法：scripts/check-size.sh [desktop|android|all] [--allow-missing]
#
# 门禁语义（QA D-5 修订）：
#   - 某平台**一个产物都没有** → 判定失败（避免"空跑即通过"，红线形同虚设）；
#     仅在显式传 --allow-missing 时才跳过。
#   - 该平台**有产物** → 逐个核验体积；不因缺少某一种打包格式而失败
#     （例如只出 NSIS 不出 MSI 是允许的）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WIN_LIMIT_MB=10
APK_LIMIT_MB=12
TARGET="all"
ALLOW_MISSING=0
for arg in "$@"; do
  case "$arg" in
    --allow-missing) ALLOW_MISSING=1 ;;
    desktop|android|all) TARGET="$arg" ;;
    *) printf '\033[1;31m[FAIL]\033[0m 未知参数：%s（可选 desktop|android|all|--allow-missing）\n' "$arg"; exit 2 ;;
  esac
done

pass() { printf '\033[1;32m[ OK ]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[FAIL]\033[0m %s\n' "$*"; }
info() { printf '\033[1;34m[size]\033[0m %s\n' "$*"; }

size_mb() { awk "BEGIN { printf \"%.2f\", $1/1048576 }"; }

# 通用核验：$1=标签 $2=体积上限MB $3=候选目录（空格分隔）
# 之后从 stdin 读取以 NUL 分隔的文件列表。
check_stream() {
  local label="$1" limit="$2" bases="$3"
  local found=0 rc=0
  # 注意：workspace 根 target/ 才是本项目实际产物目录；src-tauri/target/ 兼容单包布局。
  # shellcheck disable=SC2086
  while IFS= read -r -d '' f; do
    found=$((found + 1))
    local bytes mb
    bytes=$(stat -c%s "$f" 2>/dev/null || stat -f%z "$f")
    mb=$(size_mb "$bytes")
    if awk "BEGIN { exit !($mb <= $limit) }"; then
      pass "$label 体积达标：$f（${mb} MB ≤ ${limit} MB）"
    else
      fail "$label 体积超标：$f（${mb} MB > ${limit} MB）"
      rc=1
    fi
  done

  if [ "$found" -eq 0 ]; then
    if [ "$ALLOW_MISSING" -eq 1 ]; then
      info "未发现 $label 产物（尚未构建）。已按 --allow-missing 跳过。"
      return 0
    fi
    fail "未发现 $label 产物（尚未构建）。门禁判定为失败——请先构建，或显式加 --allow-missing 跳过。"
    return 1
  fi
  return "$rc"
}

check_desktop() {
  info "检查 Windows 安装包体积上限（≤ ${WIN_LIMIT_MB} MB）"
  check_stream "桌面安装包" "$WIN_LIMIT_MB" "target src-tauri/target" < <(
    find target src-tauri/target -type f \( -name '*.msi' -o -name '*.exe' \) -print0 2>/dev/null \
      | grep -z -e 'bundle' || true
  )
}

check_android() {
  info "检查 Android APK 体积上限（≤ ${APK_LIMIT_MB} MB）"
  check_stream "APK" "$APK_LIMIT_MB" "src-tauri/gen gen" < <(
    find src-tauri/gen gen -type f -name '*.apk' -print0 2>/dev/null || true
  )
}

rc=0
case "$TARGET" in
  desktop) check_desktop || rc=1 ;;
  android) check_android || rc=1 ;;
  all)     check_desktop || rc=1; check_android || rc=1 ;;
esac

if [ "$rc" -eq 0 ]; then
  pass "体积核验通过。"
else
  fail "体积核验未通过。"
fi
exit "$rc"

#!/usr/bin/env bash
# PasswordMS 体积核验（CI 门禁）
# Windows 安装包 ≤ 10 MB；Android APK ≤ 12 MB
# 用法：scripts/check-size.sh [desktop|android|all] [--allow-missing]
#
# 门禁语义（QA D-5 修订；D-176 补"核验的必须是交付物"）：
#   - 某平台**一个产物都没有** → 判定失败（避免"空跑即通过"，红线形同虚设）；
#     仅在显式传 --allow-missing 时才跳过。
#   - 该平台**有产物** → 逐个核验体积；不因缺少某一种打包格式而失败
#     （例如只出 NSIS 不出 MSI 是允许的）。
#   - **只核验当前版本的交付物**（版本取自 tauri.conf.json）。构建目录里躺着历代
#     产物，全扫一遍等于拿旧包凑数：今天这个包超标，门照样报"通过"。
#     Android 侧还额外区分"构建中间产物"（src-tauri/gen 下的 unsigned APK）——
#     它在盘上不算交付了包。
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
  # 【D-176】和 Android 同一个毛病：bundle/ 里躺着历代安装包（1.5.6 起），
  # 全扫一遍会拿旧包凑数——今天这个包超标，门照样报"通过"。所以按当前版本收窄。
  local app_version probe
  app_version="$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || true)"
  if [ -z "$app_version" ]; then
    fail "读不出当前版本号（src-tauri/tauri.conf.json）—— 无法确定该核验哪些安装包"
    return 1
  fi
  # 判据用「文件名里含当前版本」而不是拼死文件名：NSIS 是 `_x64-setup.exe`、
  # MSI 是 `_x64_en-US.msi`，拼死一处就会在换打包格式时静默漏检。
  # `-print -quit` 而不是 `-print0`：这里只要"有没有"，命令替换会把 NUL 吃掉并告警。
  probe=$(find target src-tauri/target -type f \( -name '*.msi' -o -name '*.exe' \) \
    -path '*bundle*' -name "PasswordMS_${app_version}_*" -print -quit 2>/dev/null || true)
  if [ -z "$probe" ]; then
    if [ "$ALLOW_MISSING" -eq 1 ]; then
      info "未发现 ${app_version} 的桌面安装包（尚未构建）。已按 --allow-missing 跳过。"
      return 0
    fi
    fail "未发现 ${app_version} 的桌面安装包。门禁判定为失败——请先构建，或显式加 --allow-missing 跳过。"
    return 1
  fi
  check_stream "桌面安装包" "$WIN_LIMIT_MB" "target src-tauri/target" < <(
    find target src-tauri/target -type f \( -name '*.msi' -o -name '*.exe' \) \
      -path '*bundle*' -name "PasswordMS_${app_version}_*" -print0 2>/dev/null || true
  )
}

check_android() {
  info "检查 Android APK 体积上限（≤ ${APK_LIMIT_MB} MB）"
  # 【D-176】这里原来扫 `src-tauri/gen gen` 找 `*.apk`，找到的是
  # `app-universal-release-unsigned.apk` —— **构建中间产物，不是发出去的包**。
  # 两个后果，一个比一个糟：
  #   ① 真正要交付的 `target/PasswordMS-<ver>-arm64.apk` 根本不在扫描范围内，
  #      体积超标它看不见；
  #   ② 中间产物在，门就"发现产物"了，于是**用错了文件也算通过** —— 报绿而红线没守。
  # 所以判据改成「发出去的那个文件」：target/ 下由 build-android.sh 签好名的 arm64 包。
  #
  # 还要**钉住版本**：target/ 里躺着历代产物（1.4.10 起二十几个），全扫一遍等于
  # 拿旧包凑数——今天这个包超标也说"通过"。版本与 build-android.sh 同源
  # （`tauri.conf.json`），不另立一份。
  local app_version apk_name
  app_version="$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || true)"
  if [ -z "$app_version" ]; then
    fail "读不出当前版本号（src-tauri/tauri.conf.json）—— 无法确定该核验哪个 APK"
    return 1
  fi
  apk_name="target/PasswordMS-${app_version}-arm64.apk"
  if [ ! -f "$apk_name" ]; then
    # 只有中间产物时要说清为什么不算，否则下一个人会以为是脚本漏扫。
    if find src-tauri/gen gen -type f -name '*.apk' -print0 2>/dev/null | grep -qz .; then
      info "只找到构建中间产物（src-tauri/gen 下的 unsigned APK），签名交付包还没生成 —— 不计入体积门。"
    fi
    if [ "$ALLOW_MISSING" -eq 1 ]; then
      info "未发现 ${app_version} 的 APK 交付物（${apk_name}）。已按 --allow-missing 跳过。"
      return 0
    fi
    fail "未发现 ${app_version} 的 APK 交付物（${apk_name}）。门禁判定为失败——请先出包，或显式加 --allow-missing 跳过。"
    return 1
  fi
  printf '%s\0' "$apk_name" | check_stream "APK" "$APK_LIMIT_MB" "target"
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

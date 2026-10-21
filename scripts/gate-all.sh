#!/usr/bin/env bash
# PasswordMS 一键质量总门（#530-W1）。
#
# ⛔ 铁律（不能靠操作者记得，脚本自己兜底，但人必须知道为什么）：
#   本机 Bash 默认 PATH 里**没有 cargo**（Rust 装在 $HOME/.cargo/bin）。
#   下面的 Rust 门若不加 `export PATH="$HOME/.cargo/bin:$PATH"` 会直接
#   「cargo: command not found」。脚本第 60 行附近统一 export，勿删。
#
# 门清单（顺序即执行顺序，逐段汇报 + 计时）：
#   1. 前端三门     npm run test / npm run check / npm run check:css
#   2. Rust 全量    cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm
#                   （timing_sidechannel 单例 106~151s 属正常耗时（KDF 烧时序），
#                     全量约 4–6 分钟，取决于是否需重新编译）
#   3. Rust 静态    cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm
#                   （0 error 阻塞；warning 数如实汇报不阻塞，当前基线 ≤7）
#   4. 体积门       bash scripts/check-size.sh desktop --allow-missing
#                   （日常门语义：本机未出包时跳过不判负；出包门在 build-android.sh
#                    与显式构建流程内，见 docs/07-quality-gate.md §2）
#                   Android：target/PasswordMS-*-arm64.apk 存在则逐个核验 ≤12MB，
#                   不存在提示「未出包，跳过」不算失败。
#   末尾汇总 PASS/FAIL + 各门耗时；任一门 FAIL → exit 1。
#   可选参数 --keep-going：某门失败后仍跑完剩余门再汇总（默认 fail-fast）。
#
# 不做的事：不跑 tauri build（太重，出包是显式动作）；不内联重写体积核验
# （直接复用 check-size.sh）。
#
# 用法：bash scripts/gate-all.sh [--keep-going]
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ⛔ 铁律：本机 Bash 默认无 cargo，必须显式补 PATH（见头注释）。
export PATH="$HOME/.cargo/bin:$PATH"

KEEP_GOING=0
for arg in "$@"; do
  case "$arg" in
    --keep-going) KEEP_GOING=1 ;;
    *) printf '\033[1;31m[FAIL]\033[0m 未知参数：%s（可选 --keep-going）\n' "$arg"; exit 2 ;;
  esac
done

pass() { printf '\033[1;32m[ OK ]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[FAIL]\033[0m %s\n' "$*"; }
info() { printf '\033[1;34m[gate]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[fatal]\033[0m %s\n' "$*" >&2; exit 1; }

# 门登记表：名称 → 结果(PASS/FAIL/SKIP) + 耗时秒。汇总段逐行打印。
RESULTS=()

record() { # $1=名称 $2=结果 $3=耗时秒 $4=备注(可空)
  RESULTS+=("$1|$2|$3|$4")
}

run_gate() { # $1=名称 $2=命令…
  local name="$1"; shift
  local t0 t1 rc
  info "▶ ${name}：$*"
  t0=$(date +%s)
  "$@" > /tmp/pms-gate-last.log 2>&1
  rc=$?
  t1=$(date +%s)
  cat /tmp/pms-gate-last.log
  if [ "$rc" -eq 0 ]; then
    pass "${name} 通过（$((t1 - t0))s）"
    record "$name" PASS "$((t1 - t0))" ""
    return 0
  fi
  fail "${name} 失败（$((t1 - t0))s，rc=${rc}）"
  record "$name" FAIL "$((t1 - t0))" "rc=${rc}"
  return 1
}

# ===== 1. 前端三门 =====
frontend_gate() {
  local rc=0
  run_gate "前端单测(npm run test)" npm run test || rc=1
  if [ "$rc" -eq 1 ] && [ "$KEEP_GOING" -eq 0 ]; then return 1; fi
  run_gate "前端类型检查(npm run check)" npm run check || rc=1
  if [ "$rc" -eq 1 ] && [ "$KEEP_GOING" -eq 0 ]; then return 1; fi
  run_gate "CSS 层叠契约(npm run check:css)" npm run check:css || rc=1
  return "$rc"
}

# ===== 2. Rust 全量测试 =====
rust_test_gate() {
  run_gate "Rust 全量(cargo test -p passwordms-core)" \
    cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm
}

# ===== 3. clippy（0 error 阻塞；warning 如实汇报不阻塞，基线 ≤7）=====
clippy_gate() {
  local t0 t1 rc warn_count
  info "▶ Rust 静态检查(cargo clippy --workspace --all-targets)"
  t0=$(date +%s)
  cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm \
    > /tmp/pms-gate-clippy.log 2>&1
  rc=$?
  t1=$(date +%s)
  # 真实 lint 计数：剔除 cargo 汇总行（"... generated N warnings"）与
  # build-script 输出（如 D-33 windres 兜底提示）——它们不是 lint，
  # 且汇总行会把同一 warning 重复计入（lib / lib test / 测试目标各一行）。
  warn_count=$(grep '^warning: ' /tmp/pms-gate-clippy.log 2>/dev/null \
    | grep -v 'generated [0-9]* warning' | grep -v 'windres' | wc -l)
  if [ "$rc" -ne 0 ]; then
    tail -30 /tmp/pms-gate-clippy.log
    fail "clippy 失败（$((t1 - t0))s，rc=${rc}）"
    record "clippy" FAIL "$((t1 - t0))" "rc=${rc}"
    return 1
  fi
  pass "clippy 0 error（warning ${warn_count} 条，非阻塞；基线 ≤7）"
  record "clippy" PASS "$((t1 - t0))" "warnings=${warn_count}"
  if [ "${warn_count:-0}" -gt 7 ]; then
    info "⚠ warning 数超过已知基线 7，请核对是否新引入（不阻塞，如实登记）"
  fi
  return 0
}

# ===== 4. 体积门 =====
size_gate() {
  local rc=0 apk f bytes mb t0 t1
  run_gate "体积门-桌面(check-size.sh desktop)" \
    bash scripts/check-size.sh desktop --allow-missing || rc=1
  if [ "$rc" -eq 1 ] && [ "$KEEP_GOING" -eq 0 ]; then return 1; fi

  # Android：signed APK 落在 target/PasswordMS-*-arm64.apk（build-android.sh 产物）。
  # check-size.sh android 只扫 src-tauri/gen 与 gen，不含 target/，故这里单独核验。
  local apks=()
  while IFS= read -r -d '' f; do apks+=("$f"); done < <(
    find target -maxdepth 1 -type f -name 'PasswordMS-*-arm64.apk' -print0 2>/dev/null
  )
  if [ "${#apks[@]}" -eq 0 ]; then
    info "体积门-Android：未出包（target/ 下无 PasswordMS-*-arm64.apk），跳过不算失败"
    record "体积门-Android" SKIP "-" "未出包"
    return "$rc"
  fi
  t0=$(date +%s)
  for f in "${apks[@]}"; do
    bytes=$(stat -c%s "$f" 2>/dev/null || stat -f%z "$f")
    mb=$(awk "BEGIN { printf \"%.2f\", $bytes/1048576 }")
    if awk "BEGIN { exit !($mb <= 12) }"; then
      pass "体积门-Android APK 体积达标：$f（${mb} MB ≤ 12 MB）"
    else
      fail "体积门-Android APK 体积超标：$f（${mb} MB > 12 MB）"
      rc=1
    fi
  done
  t1=$(date +%s)
  # ⚠ 结果必须跟着 rc 走：以前这里写死 PASS，APK 超标时上面已 fail 却仍在汇总里
  # 记一行「PASS」，汇总与真实判定互相矛盾（总判定靠返回值仍是对的，但汇总行在骗人）。
  if [ "$rc" -eq 0 ]; then
    record "体积门-Android" PASS "$((t1 - t0))" "${#apks[@]} 个产物"
  else
    record "体积门-Android" FAIL "$((t1 - t0))" "APK 超 12MB"
  fi
  return "$rc"
}

# ===== 主流程 =====
info "════ PasswordMS 一键质量总门 ════"
FAILED=0
frontend_gate  || FAILED=1
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  rust_test_gate || FAILED=1
fi
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  clippy_gate   || FAILED=1
fi
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  size_gate     || FAILED=1
fi

# ===== 汇总 =====
echo
info "════ 汇总 ════"
TOTAL_PASS=0 TOTAL_FAIL=0 TOTAL_SKIP=0
for row in "${RESULTS[@]}"; do
  IFS='|' read -r name result secs note <<< "$row"
  # 耗时字段可能是 "-"（不适用），别直接拼成 "${secs}s" → 会打印出「— -s」
  if [ "$secs" = "-" ]; then secs_disp=""; else secs_disp="${secs}s "; fi
  case "$result" in
    PASS)
      pass "${name} — ${secs_disp}${note}"
      TOTAL_PASS=$((TOTAL_PASS + 1))
      ;;
    FAIL)
      fail "${name} — ${secs_disp}${note}"
      TOTAL_FAIL=$((TOTAL_FAIL + 1))
      ;;
    SKIP)
      info "${name} — 跳过（${note}）"
      TOTAL_SKIP=$((TOTAL_SKIP + 1))
      ;;
  esac
done
echo
if [ "$TOTAL_FAIL" -eq 0 ]; then
  pass "总判定：PASS（通过 ${TOTAL_PASS} 门，跳过 ${TOTAL_SKIP} 门）"
  exit 0
fi
fail "总判定：FAIL（通过 ${TOTAL_PASS} / 失败 ${TOTAL_FAIL} / 跳过 ${TOTAL_SKIP}）"
exit 1

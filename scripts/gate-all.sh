#!/usr/bin/env bash
# PasswordMS 一键质量总门（#530-W1）。
#
# ⛔ 铁律（不能靠操作者记得，脚本自己兜底，但人必须知道为什么）：
#   本机 Bash 默认 PATH 里**没有 cargo**（Rust 装在 $HOME/.cargo/bin）。
#   下面的 Rust 门若不加 `export PATH="$HOME/.cargo/bin:$PATH"` 会直接
#   「cargo: command not found」。脚本第 60 行附近统一 export，勿删。
#
# 门清单（顺序即执行顺序，逐段汇报 + 计时）：
#   0. 版本同步门   bash scripts/check-version.sh（D-177：四处版本一致 + 锁文件可用）
#                   （秒级，放最前面：版本对不上的话后面所有数字都不属于"要发的那一版"）
#   1. 前端三门     npm run test / npm run check / npm run check:css
#   2. Rust 格式    cargo fmt --all -- --check
#                   （Q-2，2026-10-22 起纳入。秒级开销，**0 处差异才通过**；
#                     基线从 v1.2.11 拉平前的 12 处 / 3 文件降到 0。
#                     放在重活前面：格式红是最容易修的，不该让人等 6 分钟测试才发现）
#   3. Rust 全量    cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm
#                   （timing_sidechannel 单例 106~151s 属正常耗时（KDF 烧时序），
#                     全量约 4–6 分钟，取决于是否需重新编译）
#   4. Rust 静态    cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm
#                   （0 error 阻塞；warning 数如实汇报不阻塞，基线 0——#530 清零后
#                     任何 warning 都视为新引入，见 docs/07-quality-gate.md §1.4）
#   5. 体积门       bash scripts/check-size.sh desktop|android --allow-missing
#                   （日常门语义：本机未出包时跳过不判负；出包门在 build-android.sh
#                    与显式构建流程内，见 docs/07-quality-gate.md §2）
#                   **只核验当前版本的交付物**（D-176）：构建目录里躺着历代产物，
#                   全扫一遍等于拿旧包凑数。Android 交付物 =
#                   target/PasswordMS-<ver>-arm64.apk（src-tauri/gen 下的 unsigned
#                   APK 是中间产物，不算出了包）。
#
# 另外两件贯穿全轮的事：
#   · **锚自检**：开头快照 HEAD + 工作区，结束再快照一次。期间有改动 → 本轮数字
#     不可引用（这是安全评审 2026-11-03 自己踩的坑：前端三门跑完才改 i18n，
#     报出来的"三门全绿"锚在当天那份文件上，但引用时写的是提交号）。
#     它检不出"跑之前就是脏的"——那种情形改为在报告里如实标注。
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

# ===== 锚：本轮所有数字的前提 =====
#
# 粒度是「门 × 输入文件」，不是「提交 × 全部数字」（安全评审 2026-11-03 自我更正）：
# 他报"前端三门全绿"时，vitest 是最后跑的，而四份 i18n 在它之前已经被改过——
# 那三门锚的是当时的工作区，却被当成某个提交号的结论引用。
#
# 这里能自动检出的只有一种：**门跑期间文件又变了**。检出不了的另一种（跑之前就脏）
# 不装作能检：改为在报告开头如实打印，让引用数字的人知道它对应的是"工作区"而非提交。
anchor_snapshot() {
  local head dirty
  head=$(git rev-parse HEAD 2>/dev/null || echo "no-git")
  dirty=$(git status --porcelain 2>/dev/null | LC_ALL=C sort)
  printf 'HEAD %s\n%s\n' "$head" "$dirty"
}

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

# ===== 2. rustfmt 格式基线（Q-2：0 处差异才通过）=====
#
# 为什么设成阻塞门：拉平前的 `--check` 在本机长期为红（v1.2.11 实测 12 处 / 3 文件），
# 靠「记得顺手跑一下」守不住 —— 和 D-33 那条「前置条件必须写进脚本」同一条原则。
# 输出格式：rustfmt 每处差异打印 `Diff in <路径>:<起行>:`，故用 ^Diff in 计数。
# `grep -c` 在 0 匹配时返回 1，必须 `|| echo 0` 兜底，否则差别数会变成空串。
fmt_gate() {
  local t0 t1 rc diffs
  info "▶ Rust 格式基线(cargo fmt --all -- --check)"
  t0=$(date +%s)
  cargo fmt --all -- --check > /tmp/pms-gate-fmt.log 2>&1
  rc=$?
  t1=$(date +%s)
  if [ "$rc" -ne 0 ]; then
    diffs=$(grep -c '^Diff in' /tmp/pms-gate-fmt.log 2>/dev/null || echo 0)
    # 只列前 20 行坐标，不刷屏；完整 diff 见 /tmp/pms-gate-fmt.log
    grep '^Diff in' /tmp/pms-gate-fmt.log 2>/dev/null \
      | sed 's|.*PasswordMS.||' | head -20
    fail "fmt 失败（$((t1 - t0))s，${diffs} 处差异）—— 跑 cargo fmt --all 后重试"
    record "fmt" FAIL "$((t1 - t0))" "diffs=${diffs}"
    return 1
  fi
  pass "fmt 0 处差异（$((t1 - t0))s）"
  record "fmt" PASS "$((t1 - t0))" ""
  return 0
}

# ===== 3. Rust 全量测试 =====
rust_test_gate() {
  run_gate "Rust 全量(cargo test -p passwordms-core)" \
    cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm
}

# ===== 4. clippy（0 error 阻塞；warning 如实汇报不阻塞，基线 0）=====
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
  pass "clippy 0 error（warning ${warn_count} 条，非阻塞；基线 0）"
  record "clippy" PASS "$((t1 - t0))" "warnings=${warn_count}"
  if [ "${warn_count:-0}" -gt 0 ]; then
    info "⚠ warning 数超过已知基线 0，请核对是否新引入（不阻塞，如实登记）"
  fi
  return 0
}

# ===== 5. 体积门 =====
# ===== 0. 版本同步门（D-177）=====
#
# 为什么排在最前：版本对不上的话，后面每一门跑出来的数字都不属于"要发的那一版"，
# 先花 6 分钟跑完再报错，等于让人白等一轮。这一门是秒级的。
version_gate() {
  run_gate "版本同步门(check-version.sh)" bash scripts/check-version.sh
}

size_gate() {
  local rc=0 apk_name app_version t0 t1
  run_gate "体积门-桌面(check-size.sh desktop)" \
    bash scripts/check-size.sh desktop --allow-missing || rc=1
  if [ "$rc" -eq 1 ] && [ "$KEEP_GOING" -eq 0 ]; then return 1; fi

  # Android：交付物 = target/PasswordMS-<当前版本>-arm64.apk（build-android.sh 签好名的包）。
  #
  # D-176：这段原来内联着体积计算，因为 check-size.sh 的 android 分支只扫
  # `src-tauri/gen` 与 `gen` —— 那儿只有 **unsigned 的构建中间产物**。两个后果：
  # 真正的交付包不在扫描范围（超标看不见），而中间产物在盘上就算"发现产物"，
  # 于是**用错了文件也报绿**。现在判据收归 check-size.sh 一处，这里只留
  # "没出包就记 SKIP"这一句（不重复任何体积逻辑，也不重复任何文件名规则）。
  app_version="$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || true)"
  apk_name="target/PasswordMS-${app_version}-arm64.apk"
  if [ -z "$app_version" ] || [ ! -f "$apk_name" ]; then
    info "体积门-Android：未出包（无 ${apk_name:-未知版本的 APK}），跳过不算失败"
    record "体积门-Android" SKIP "-" "未出包"
    return "$rc"
  fi
  t0=$(date +%s)
  run_gate "体积门-Android(check-size.sh android)" \
    bash scripts/check-size.sh android || rc=1
  t1=$(date +%s)
  # ⚠ 汇总行必须跟着 rc 走：以前这里写死 PASS，超标时上面已 fail 却仍记一行「PASS」。
  if [ "$rc" -eq 0 ]; then
    record "体积门-Android" PASS "$((t1 - t0))" "$apk_name"
  else
    record "体积门-Android" FAIL "$((t1 - t0))" "$apk_name"
  fi
  return "$rc"
}

# ===== 主流程 =====
info "════ PasswordMS 一键质量总门 ════"

# 锚：先记下这一轮跑在什么上面。脏工作区不拦——开发中天天是脏的，
# 拦了就没人跑门——但必须**说出来**，否则下一个人会把这里的数字当成某个提交的结论。
ANCHOR_BEFORE="$(anchor_snapshot)"
ANCHOR_HEAD="$(printf '%s\n' "$ANCHOR_BEFORE" | head -1 | sed 's/^HEAD //')"
ANCHOR_DIRTY="$(printf '%s\n' "$ANCHOR_BEFORE" | tail -n +2 | grep -c . || true)"
if [ "${ANCHOR_DIRTY:-0}" -eq 0 ]; then
  info "锚：HEAD ${ANCHOR_HEAD}（工作区干净）"
else
  info "锚：HEAD ${ANCHOR_HEAD} + 工作区 ${ANCHOR_DIRTY} 项未提交改动"
  info "    ⇒ 本轮数字对应的是**当前工作区**，不是提交 ${ANCHOR_HEAD} 的结论。"
fi

FAILED=0
version_gate   || FAILED=1
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  frontend_gate  || FAILED=1
fi
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  fmt_gate      || FAILED=1
fi
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  rust_test_gate || FAILED=1
fi
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  clippy_gate   || FAILED=1
fi
if [ "$FAILED" -eq 0 ] || [ "$KEEP_GOING" -eq 1 ]; then
  size_gate     || FAILED=1
fi

# ===== 锚自检：门跑期间文件变过没有 =====
# 这一条只能检出"门跑期间被改"。跑之前就脏的情形已在开头打印，不在这里假装能检。
ANCHOR_AFTER="$(anchor_snapshot)"
if [ "$ANCHOR_BEFORE" = "$ANCHOR_AFTER" ]; then
  pass "锚自检通过：整轮门禁期间 HEAD 与工作区未变（数字可引用）"
  record "锚自检" PASS "-" "HEAD ${ANCHOR_HEAD}"
else
  fail "锚自检失败：门跑期间 HEAD 或工作区发生了变化 —— 本轮数字**不可**整轮引用"
  printf '%s\n' "$ANCHOR_BEFORE" > /tmp/pms-anchor-before.txt
  printf '%s\n' "$ANCHOR_AFTER"  > /tmp/pms-anchor-after.txt
  diff -u /tmp/pms-anchor-before.txt /tmp/pms-anchor-after.txt || true
  record "锚自检" FAIL "-" "门跑期间目录被改"
  FAILED=1
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

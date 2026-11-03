#!/usr/bin/env bash
# 版本同步门（D-177）：版本号是**四处**，不是三处。
#
# 为什么要有这道门：1.5.29 出包那一轮 `bfc3a22` 只改了前三处，`Cargo.lock` 里
# `passwordms` / `passwordms-core` 还停在 1.5.28。产物没问题（构建自己会改锁），
# 坏的是另外两件事：
#   ① **可复现性**——干净检出 `cargo build --locked` 会直接失败（锁与清单不符）；
#   ② **复验前提**——发布提交自己把工作区弄脏，下一轮门禁的"锚"就对不上了。
# 这两条都不会在构建时报警，所以必须有一道门替人记得。
#
# 门做两件事：
#   1. 读四处版本，**全部相等**才通过（逐处打印，不一致时指名道姓）；
#   2. `cargo metadata --locked --offline` 必须成功 —— 这一条才真正证明
#      「锁文件与清单一致」，光比字符串比不出 `--locked` 会不会炸。
#
# 用法：bash scripts/check-version.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export PATH="$HOME/.cargo/bin:$PATH"

pass() { printf '\033[1;32m[ OK ]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[FAIL]\033[0m %s\n' "$*"; }
info() { printf '\033[1;34m[ver]\033[0m %s\n' "$*"; }

read_pkg_json() {
  node -p "require('./package.json').version" 2>/dev/null || true
}
read_tauri_conf() {
  node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || true
}
# 只在 `[workspace.package]` 段内取 version —— 根 Cargo.toml 里还有 `[package]`
# 之外的其它 `version =`（依赖版本），盲取会拿到第一个碰上的。
read_workspace_version() {
  awk '
    /^\[workspace\.package\]/ { f = 1; next }
    /^\[/ { f = 0 }
    f && /^version[[:space:]]*=/ { v = $3; gsub(/"/, "", v); print v; exit }
  ' Cargo.toml
}
# 锁文件里的版本：按包名定位，读到同一条目内的 version 为止（不跨到下一个 [[package]]）。
read_lock_version() {
  awk -v name="$1" '
    $0 == "name = \"" name "\"" { f = 1; next }
    f && /^\[\[package\]\]/ { f = 0 }
    f && /^version[[:space:]]*=/ { v = $3; gsub(/"/, "", v); print v; exit }
  ' Cargo.lock
}

V_PKG=$(read_pkg_json)
V_TAURI=$(read_tauri_conf)
V_WS=$(read_workspace_version)
V_LOCK_APP=$(read_lock_version "passwordms")
V_LOCK_CORE=$(read_lock_version "passwordms-core")

declare -a NAMES=(
  "package.json"
  "src-tauri/tauri.conf.json"
  "Cargo.toml [workspace.package]"
  "Cargo.lock: passwordms"
  "Cargo.lock: passwordms-core"
)
declare -a VALUES=("$V_PKG" "$V_TAURI" "$V_WS" "$V_LOCK_APP" "$V_LOCK_CORE")

rc=0
REF="${VALUES[0]}"
for i in "${!NAMES[@]}"; do
  name="${NAMES[$i]}"
  value="${VALUES[$i]}"
  if [ -z "$value" ]; then
    fail "${name}：读不出版本号（文件缺失或格式变了）"
    rc=1
    continue
  fi
  if [ "$value" = "$REF" ]; then
    pass "${name} = ${value}"
  else
    fail "${name} = ${value}（与 package.json 的 ${REF} 不一致）"
    rc=1
  fi
done

if [ "$rc" -ne 0 ]; then
  fail "版本不一致。四处必须同时改：package.json / src-tauri/tauri.conf.json /"
  fail "Cargo.toml [workspace.package] / Cargo.lock（改完跑一次 cargo check 刷新锁）"
  exit 1
fi

# 第二条：字符相等 ≠ 锁文件可用。`--locked` 会拒绝任何需要改锁的构建，
# 这正是「干净检出能不能编译」的判据；`--offline` 保证这道门不依赖网络。
info "核对 cargo metadata --locked --offline（锁文件与清单是否真的一致）"
if ! cargo metadata --locked --offline --format-version 1 > /tmp/pms-version-metadata.json 2>/tmp/pms-version-metadata.err; then
  fail "cargo metadata --locked 失败 —— 锁文件与清单不一致（改过 Cargo.toml 却没刷新锁？）"
  sed -n '1,12p' /tmp/pms-version-metadata.err
  exit 1
fi

pass "版本同步门通过：四处一致（${REF}），锁文件可用。"
exit 0

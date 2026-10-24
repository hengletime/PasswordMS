#!/usr/bin/env bash
# PasswordMS Android APK 构建 + 完整性校验 + 自动修复（缺陷 D-17；方案：上游路径 + 校验 + 自动修复）
#
# 前置：Rust（含 aarch64-linux-android 目标）+ Android SDK/NDK + Node + Tauri CLI。
#
# 背景（本机已用**原生工具**实测确认）：
#   `npx tauri android build --apk --target aarch64` 会产出一个「空壳 APK」——
#   包内 lib/<abi>/libpasswordms_lib.so 长度为 0，即**没有原生代码**。
#
#   机制：Tauri 经 cargo-mobile2 要把 Rust 产物放进 jniLibs/<abi>/，做法是**创建符号链接**。
#   构建日志可直接看到该动作：
#       Info symlinking lib ".../libpasswordms_lib.so" in jniLibs dir ".../jniLibs/arm64-v8a"
#
#   ⚠️ 根因：**本机进程无法创建符号链接**（缺 SeCreateSymbolicLinkPrivilege）。
#      原生证据 —— 原生 PowerShell 在三个位置全部失败：
#        New-Item -ItemType SymbolicLink
#          → UnauthorizedAccessException / HRESULT=0x80070005 / "此操作需要管理员权限。"
#      同时：AppModelUnlock 注册表键不存在（= 开发者模式**未开启**），IsAdmin=False。
#      符号链接创建被拒后，jniLibs 里留下一个 **0 字节普通文件**。
#
#      产物形态原生取证（Attributes=Archive / IsReparsePoint=False / LinkType 为空 /
#      可打开、可读长度 0）→ 它**不是符号链接、也不是断链**，就是个真实的 0 字节文件。
#      AGP 把这个空文件打进 APK。
#
#   ⚠️ 方法论坑（踩过，勿重蹈）：判断"能不能建符号链接"**不能**用本机 Bash 工具里的
#      Python `os.symlink` —— 该 shell 带文件系统垫片，`os.symlink` 会**既不抛异常、
#      也什么都不创建**（静默空操作）。曾据此误判"本机可以建链接、开开发者模式无效"，
#      该结论已作废。涉及系统级能力必须用**原生进程**（直接跑 powershell.exe）取证；
#      且"调用没报错"**不等于**"操作成功"，判定成功必须验证产物本身。
#
#   因此**绝不能用上游命令的退出码判定成功**，必须解包校验包内 .so：
#   空壳包体积天然更小，体积门禁反而更容易放行（实测空壳 1.62 MB vs 正常 6.29 MB）。
#
# 本脚本流程：
#   前置检查 → 前端构建 → 上游构建（可能产空壳）→ 解包校验 → 失败则自动修复 → 二次校验 → 体积核验
#
# 修复路径（仅在校验失败时触发）：
#   复用上游已产出的真实库 target/aarch64-linux-android/release/libpasswordms_lib.so，
#   硬拷贝进 jniLibs，再直接跑 Gradle 打包并 -x 掉全部 rustBuild* 任务（否则它们会把 jniLibs
#   再次覆盖成 0 字节）。
#   ⚠️ 修复路径**绝不**自行调用 `cargo build --target aarch64-linux-android`：该目标的链接器由
#      Tauri 在构建时以环境变量注入，.cargo/config.toml 里没有；自行调用会因找不到链接器而失败。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APK_LIMIT_MB=12
JNILIBS_DIR="src-tauri/gen/android/app/src/main/jniLibs"
ANDROID_GEN_DIR="src-tauri/gen/android"
REAL_SO="target/aarch64-linux-android/release/libpasswordms_lib.so"
BUNDLE_ABI="arm64-v8a"

# 日志颜色：沿用 [android] 蓝 / [warn ] 黄 / [fatal] 红，新增 [ OK ] 绿 / [FAIL] 红
C_BLUE='\033[1;34m'; C_YEL='\033[1;33m'; C_RED='\033[1;31m'; C_GRN='\033[1;32m'; C_RST='\033[0m'
log()  { printf "${C_BLUE}[android]${C_RST} %s\n" "$*"; }
warn() { printf "${C_YEL}[warn ]${C_RST} %s\n" "$*"; }
ok()   { printf "${C_GRN}[ OK ]${C_RST} %s\n" "$*"; }
fail() { printf "${C_RED}[FAIL]${C_RST} %s\n" "$*"; }
die()  { printf "${C_RED}[fatal]${C_RST} %s\n" "$*" >&2; exit 1; }

WORK_DIR="$(mktemp -d)"
# 某些 MSYS/Cygwin 环境里 mktemp 会返回 Windows 混合路径（形如 C:\...\Temp/tmp.XXXX）：
# 该路径含「内嵌盘符」，会被部分环境的 safe-delete 守卫拦截，导致 rm 全部失败（临时目录清理不掉）。
# 若可用 cygpath，则统一转成 POSIX 路径（/tmp/...）。非 MSYS 环境无 cygpath，此处为无操作。
if command -v cygpath >/dev/null 2>&1; then
  WORK_DIR="$(cygpath -u "$WORK_DIR")"
fi
# 清理临时目录：显式忽略失败（无论守卫是否拦截 rm，都不能让清理步骤影响脚本退出码）
trap 'rm -rf -- "$WORK_DIR" >/dev/null 2>&1 || true' EXIT

# ---------- 0. 前置检查 ----------
for c in cargo npm npx; do
  command -v "$c" >/dev/null 2>&1 || die "缺少依赖：$c（请先安装 Rust 工具链 / Node.js 后重试）"
done

if [ -z "${JAVA_HOME:-}" ]; then
  # 前置条件自愈（2026-10-24，同 D-33 windres 兜底精神）：构建的前置必须写进脚本，
  # 不能靠操作者记得 export。按常见位置探测 JDK 17，探测不到才报错。
  for cand in "D:/AndroidDev/jdk17" "$HOME/.workbuddy/tools/jdk17" "C:/Program Files/Java/jdk-17" "C:/Program Files/Microsoft/jdk-17"; do
    if [ -d "$cand" ]; then
      export JAVA_HOME="$cand"
      log "JAVA_HOME 未设置，已自动探测：$JAVA_HOME"
      break
    fi
  done
  [ -n "${JAVA_HOME:-}" ] || die "未设置 JAVA_HOME（Android/Gradle 需要 JDK 17）。示例：export JAVA_HOME=\"D:\\\\AndroidDev\\\\jdk17\""
fi
if [ -z "${ANDROID_HOME:-}" ] && [ -z "${NDK_HOME:-}" ]; then
  for cand in "D:/AndroidDev/sdk" "$HOME/AppData/Local/Android/Sdk"; do
    if [ -d "$cand" ]; then
      export ANDROID_HOME="$cand"
      log "ANDROID_HOME 未设置，已自动探测：$ANDROID_HOME"
      break
    fi
  done
  [ -n "${ANDROID_HOME:-}" ] || die "未设置 ANDROID_HOME 或 NDK_HOME（Android SDK/NDK 路径）。示例：export ANDROID_HOME=\"D:\\\\AndroidDev\\\\sdk\""
fi
if [ -z "${ANDROID_HOME:-}" ]; then warn "仅设置了 NDK_HOME，未设置 ANDROID_HOME；若 Gradle 找不到 SDK 请补上。"; fi
if [ -z "${NDK_HOME:-}" ];    then warn "未设置 NDK_HOME；若 Rust 交叉编译找不到链接器请补上。"; fi

# 解包工具：至少要有一种，否则无法校验包内 .so
HAVE_EXTRACT=0
if command -v unzip >/dev/null 2>&1; then HAVE_EXTRACT=1; fi
if command -v python >/dev/null 2>&1 || command -v python3 >/dev/null 2>&1; then HAVE_EXTRACT=1; fi
if command -v tar >/dev/null 2>&1; then HAVE_EXTRACT=1; fi
if [ "$HAVE_EXTRACT" -eq 0 ]; then
  die "缺少解包工具：需要 unzip、python 或 tar 之一用于 APK 条目校验。请安装其一后重试。"
fi

# 读字节工具：od 优先，回退 xxd / python
HAVE_MAGIC=0
if command -v od >/dev/null 2>&1 || command -v xxd >/dev/null 2>&1 \
   || command -v python >/dev/null 2>&1 || command -v python3 >/dev/null 2>&1; then
  HAVE_MAGIC=1
fi
if [ "$HAVE_MAGIC" -eq 0 ]; then
  warn "缺少读取字节的工具（od / xxd / python 皆不可用）：ELF magic 断言将无法执行（不会静默降级——校验会判失败）。"
fi

# 哈希工具：第 ④ 条断言（包内 .so 与构建产物内容一致）**必需**。
# 这里刻意 die 而不是降级：一旦静默降级成「只查存在/非空/ELF 头」，
# 「头正确、内容任意」的坏包就会被放行 —— 那正是本条断言要堵的洞（QA 复验 F3）。
if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1 \
   && ! command -v python >/dev/null 2>&1 && ! command -v python3 >/dev/null 2>&1; then
  die "缺少计算 sha256 的工具（需要 sha256sum、shasum 或 python 之一）：无法执行「包内 .so 与构建产物逐字节一致」这条断言。本门禁不允许静默降级。"
fi

log "工具：unzip=$(command -v unzip || echo 无)  python=$(command -v python || command -v python3 || echo 无)  tar=$(command -v tar || echo 无)  od=$(command -v od || echo 无)  sha256=$(command -v sha256sum || command -v shasum || echo 无)"

# ---------- 1. Rust Android 目标 ----------
log "确保 Android 目标已安装"
rustup target add aarch64-linux-android 2>/dev/null || warn "rustup target add 失败，请手动确认 aarch64-linux-android 已安装"

# ---------- 2. 前端构建 ----------
log "构建前端"
if [ -d node_modules ]; then
  npm run build
else
  npm install && npm run build
fi

# ---------- 3. 上游构建 ----------
# 记录上游构建起点时间戳（用于修复分支的真身新鲜度门，D-60 补丁）。
# 背景（2026-10-19 实测事故）：上游 cargo 因 .cargo-build-lock os error 5 失败时，
# 旧一轮留下的 REAL_SO 仍满足「四断言」（哈希就是跟它自己比的）→ 旧 core 被打进包
# 且全部门禁绿灯——fail-open。故修复分支必须证明 REAL_SO 比上游起点新。
# ⚠️ 必须在上游构建**之前**取值（首版误放在校验节，把「编译完成时刻」误判成过期）。
UPSTREAM_START_TS="$(date +%s)"
log "上游构建：npx tauri android build --apk --target aarch64"
log "注意：本步骤在本机必然产出「空壳包」（包内 .so 为 0 字节），不得据其退出码判定成功。"
UPSTREAM_RC=0
npx tauri android build --apk --target aarch64 || UPSTREAM_RC=$?
if [ "$UPSTREAM_RC" -ne 0 ]; then
  warn "上游构建退出码=$UPSTREAM_RC（不据此判定成败，继续按 APK 实际内容校验）"
fi

# ---------- 4. 定位 APK ----------
locate_apk() {
  # 优先锁定 universal/release 下的正式产物。`find ... | head -n 1` 在同目录还存在
  # debug 包（或将来多出别的变体）时顺序不确定，可能校验到错误的 APK。
  local exact
  exact="$(find "$ANDROID_GEN_DIR/app/build/outputs/apk/universal/release" \
             -name '*.apk' -type f 2>/dev/null | head -n 1 || true)"
  if [ -n "$exact" ]; then
    printf '%s\n' "$exact"
    return 0
  fi
  # 回退：产物目录结构与预期不符时仍尽量工作
  # （head 提前退出可能让 find 收到 SIGPIPE；配合 pipefail 会返回非零，故兜底 || true）
  find "$ANDROID_GEN_DIR" -name '*.apk' -type f 2>/dev/null | head -n 1 || true
}

APK="$(locate_apk)"
if [ -z "$APK" ]; then
  die "未找到 APK 产物，请检查 Android 构建输出（$ANDROID_GEN_DIR）。"
fi
log "APK 路径：$APK"

# ---------- 5. 校验辅助 ----------
# 从 jniLibs 推导期望的包内条目：<abi>/<name>.so → lib/<abi>/<name>.so
list_expected_entries() {
  local so abi name
  for so in "$JNILIBS_DIR"/*/*.so; do
    [ -e "$so" ] || continue
    abi="$(basename "$(dirname "$so")")"
    name="$(basename "$so")"
    printf 'lib/%s/%s\n' "$abi" "$name"
  done
}

# 读取文件前 4 字节的十六进制（小写、无分隔）。无可用工具时返回非零。
read_magic() {
  local f="$1" py
  if command -v od >/dev/null 2>&1; then
    head -c 4 "$f" | od -An -tx1 | tr -d ' \n'
  elif command -v xxd >/dev/null 2>&1; then
    head -c 4 "$f" | xxd -p | tr -d '\n'
  elif command -v python >/dev/null 2>&1 || command -v python3 >/dev/null 2>&1; then
    py="$(command -v python || command -v python3)"
    "$py" -c 'import sys;sys.stdout.write(open(sys.argv[1],"rb").read(4).hex())' "$f"
  else
    return 1
  fi
}

# 解出 APK 内单个条目到 $out；多工具按序回退（unzip → python zipfile → tar/bsdtar）。
# 成功且文件非空返回 0（所用工具名写入全局 EXTRACT_TOOL）；否则返回 1。
EXTRACT_TOOL=""
extract_entry() {
  local apk="$1" entry="$2" out="$3" py tdir
  EXTRACT_TOOL=""
  rm -f "$out"

  # 1) unzip
  if command -v unzip >/dev/null 2>&1; then
    if unzip -p "$apk" "$entry" > "$out" 2>/dev/null && [ -s "$out" ]; then
      EXTRACT_TOOL="unzip"; return 0
    fi
  fi

  # 2) python zipfile
  py=""
  if command -v python >/dev/null 2>&1; then py="$(command -v python)"
  elif command -v python3 >/dev/null 2>&1; then py="$(command -v python3)"
  fi
  if [ -n "$py" ]; then
    if "$py" -c 'import sys, zipfile
apk, entry, out = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    with zipfile.ZipFile(apk) as z:
        data = z.read(entry)
except Exception:
    sys.exit(2)
open(out, "wb").write(data)' "$apk" "$entry" "$out" 2>/dev/null && [ -s "$out" ]; then
      EXTRACT_TOOL="python"; return 0
    fi
  fi

  # 3) tar（Windows 自带 bsdtar 可读 zip）
  if command -v tar >/dev/null 2>&1; then
    tdir="$(mktemp -d "$WORK_DIR/tar.XXXXXX")"
    if tar -xf "$apk" -C "$tdir" "$entry" 2>/dev/null && [ -s "$tdir/$entry" ]; then
      cp -f "$tdir/$entry" "$out"
      EXTRACT_TOOL="tar"; return 0
    fi
  fi

  return 1
}

# 计算文件 sha256（全小写十六进制）。按序回退：sha256sum → shasum → python。
# 全部不可用时返回非零（脚本前置检查已保证不会走到这里）。
# 注意：一律**从 stdin 读**，避免 coreutils 因文件名含反斜杠（Windows 路径）而在哈希前加 `\`。
sha256_of() {
  local f="$1" py
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum < "$f" | awk '{print $1}'
    return 0
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 < "$f" | awk '{print $1}'
    return 0
  fi
  py=""
  if command -v python >/dev/null 2>&1; then
    py="$(command -v python)"
  elif command -v python3 >/dev/null 2>&1; then
    py="$(command -v python3)"
  fi
  if [ -n "$py" ]; then
    "$py" -c 'import hashlib,sys;sys.stdout.write(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$f"
    return 0
  fi
  return 1
}

# 校验 APK：逐个期望条目断言
#   ① 条目存在（能解出来）  ② 长度 > 0  ③ 前 4 字节为 ELF(7f454c46)
#   ④ 内容 sha256 与本次构建产物 target/.../libpasswordms_lib.so 逐字节一致
# 四条缺一不可，且**任一条无法执行即判失败**（fail-closed，不静默降级）。
# 第 ④ 条是 QA 复验时提出的补强：只查「非空 + ELF 头」挡不住「ELF 头正确但内容不对」
# 的包（例如打进了别的库、或只被翻转了中间若干个字节）。
# 全部通过返回 0 并打印各条目字节数与 sha256；任一失败打印 [FAIL] 并返回 1。
verify_apk() {
  local apk="$1" rc=0 entry out size magic sha real_sha
  if [ ! -f "$apk" ]; then
    fail "APK 不存在：$apk"
    return 1
  fi

  local entries
  entries="$(list_expected_entries)"
  if [ -z "$entries" ]; then
    fail "无法从 jniLibs 推导出期望条目（$JNILIBS_DIR 下没有 *.so）"
    return 1
  fi

  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    out="$WORK_DIR/extract.so"
    if ! extract_entry "$apk" "$entry" "$out"; then
      fail "条目缺失或为空：$entry（unzip/python/tar 均未解出内容）"
      rc=1
      continue
    fi
    size="$(wc -c < "$out" | tr -d '[:space:]')"
    magic="$(read_magic "$out" || true)"
    # 说明：extract_entry 已要求解出的文件非空（[ -s ]），故这条正常情况下不会触发；
    # 保留它属于「即便有人改了 extract_entry 的判空逻辑，这里也不会放行空文件」的防御。
    if [ "${size:-0}" -le 0 ]; then
      fail "条目长度为 0：$entry（解包工具=${EXTRACT_TOOL}）"
      rc=1
      continue
    fi
    if [ "$magic" != "7f454c46" ]; then
      fail "条目非 ELF（magic=${magic:-空}）：$entry（解包工具=${EXTRACT_TOOL}）"
      rc=1
      continue
    fi

    # ④ 哈希一致性：包内 .so 必须与本次构建产物逐字节相同
    #    三者任一不成立都判失败，绝不退化成「只查前三条」。
    if [ ! -f "$REAL_SO" ]; then
      fail "无法完成内容一致性断言：构建产物不存在（$REAL_SO）"
      fail "    → 请确认上游构建是否真的完成了 Rust 编译。本门禁不允许降级为「只查存在/非空/ELF」。"
      rc=1
      continue
    fi
    if ! sha="$(sha256_of "$out")" || [ -z "$sha" ]; then
      fail "无法计算包内 .so 的 sha256：$entry"
      rc=1
      continue
    fi
    if ! real_sha="$(sha256_of "$REAL_SO")" || [ -z "$real_sha" ]; then
      fail "无法计算构建产物的 sha256：$REAL_SO"
      rc=1
      continue
    fi
    if [ "$sha" != "$real_sha" ]; then
      fail "条目内容与构建产物不一致：$entry"
      fail "    包内     sha256 = $sha"
      fail "    构建产物 sha256 = $real_sha  （$REAL_SO）"
      rc=1
      continue
    fi
    # 只有四条断言全部真正执行且通过，才打印这个后缀。
    # （这个后缀是「第 ④ 条已执行」的凭据，绝不能在跳过的情形下打印 —— 见 QA 复验 F3。）
    ok "$entry  字节=$size  magic=$magic  工具=$EXTRACT_TOOL  sha256=$sha  已比对构建产物（4/4）"
  done <<< "$entries"

  return "$rc"
}

# ---------- 6. 校验 →（必要时）自动修复 → 二次校验 ----------
if [ "$UPSTREAM_RC" -ne 0 ]; then
  # 毒包二号路径堵截（D-62 补丁，2026-10-20 实测）：上游 cargo 失败（如构建锁冲突）时
  # symlink 不会执行，jniLibs 里残留上一轮修复分支拷入的**真 .so**——gradle 照常打包、
  # verify_apk 四断言对旧 .so 自比全绿，包看起来完全合法，实际是旧 core。
  # 因此上游 RC≠0 时，无论 APK 校验是否通过都不可信，必须 die 重跑。
  die "上游构建退出码=${UPSTREAM_RC} 非 0——本轮 APK 内容来源不可信（jniLibs 可能残留上一轮真身）。最常见根因：.cargo-build-lock 冲突（os error 5）。请确认无并发构建/清理残留锁后重跑。本门禁不允许用来源不明的包出厂。"
fi
if verify_apk "$APK"; then
  log "APK 完整性校验通过（上游路径，无需修复）。"
else
  warn "APK 完整性校验失败：上游产出空壳包 → 进入自动修复分支（D-17）。"
  if [ ! -f "$REAL_SO" ]; then
    die "上游未产出真实库：$REAL_SO 不存在。请先确认 'npx tauri android build' 是否真的完成了 Rust 编译。（本脚本不会自行调用 cargo：Android 链接器由 Tauri 注入，直接调 cargo 会失败。）"
  fi
  # 真身新鲜度门（D-60 补丁）：REAL_SO 的 mtime 必须晚于上游构建起点，
  # 否则说明上游 cargo 没有真正编译（如构建锁冲突），拷进去的是上一轮的旧库。
  REAL_SO_TS="$(stat -c %Y "$REAL_SO" 2>/dev/null || echo 0)"
  if [ "$REAL_SO_TS" -lt "$UPSTREAM_START_TS" ]; then
    die "真身过期（stale artifact）：$REAL_SO 的 mtime（${REAL_SO_TS}）早于上游构建起点（${UPSTREAM_START_TS}）——上游 cargo 未完成本次编译。最常见根因：.cargo-build-lock 冲突（os error 5）。请清理残留锁/确认无并发构建后重跑。本门禁不允许用旧库打新包。"
  fi
  REAL_SO_BYTES="$(wc -c < "$REAL_SO" | tr -d '[:space:]')"
  log "真实库：$REAL_SO（${REAL_SO_BYTES} 字节）→ 硬拷贝进 $JNILIBS_DIR/$BUNDLE_ABI/"
  mkdir -p "$JNILIBS_DIR/$BUNDLE_ABI"
  cp -f "$REAL_SO" "$JNILIBS_DIR/$BUNDLE_ABI/"

  log "Gradle 重新打包（-x 跳过全部 rustBuild* 任务，避免其把 jniLibs 再次覆盖为 0 字节）"
  if ! ( cd "$ANDROID_GEN_DIR" && ./gradlew :app:assembleUniversalRelease \
          -x rustBuildUniversalRelease -x rustBuildArm64Release -x rustBuildArmRelease \
          -x rustBuildX86Release -x rustBuildX86_64Release --console=plain ); then
    die "Gradle 重新打包失败（见上方 Gradle 输出）。"
  fi

  APK="$(locate_apk)"
  if [ -z "$APK" ]; then
    die "修复后仍未找到 APK（$ANDROID_GEN_DIR）。"
  fi
  log "修复后 APK 路径：$APK"

  if verify_apk "$APK"; then
    log "修复后二次校验通过。（提示：此分支因上游 symlink 缺陷而存在，若 Tauri 修复后可移除。）"
  else
    die "修复后二次校验仍失败：APK=$APK。请检查上面的 [FAIL] 条目。"
  fi
fi

# ---------- 7. 签名（D-45；2026-10-20 起默认正式证书）----------
# 根因：Android 12+ 拒装未签名包，报「解析软件包时出现问题 / packageinfo is null」。
# 正式证书：passwordms-release.jks（RSA4096 / 30 年，2026-10-20 建）。
#   口令绝不进仓库——从本地私有文件 release.pass（仓库外，单行明文）读取。
#   ⚠️ keystore 文件 + release.pass 必须双备份：丢失 = 该签名永久无法再升级。
#   ⚠️ 公开发布的包一律用正式证书。测试构建时用旧测试证书显式覆盖：
#     PMS_KEYSTORE=D:/AndroidDev/keystores/passwordms-test.jks \
#     PMS_KEY_ALIAS=passwordms PMS_KEY_PASS=PmsTest2026Key bash scripts/build-android.sh
log "APK 签名（未签名包 Android 12+ 必报解析错误，D-45）"
BT_DIR="$(ls -d "${ANDROID_HOME:-D:/AndroidDev/sdk}"/build-tools/* 2>/dev/null | sort -V | tail -n 1)"
if [ -z "$BT_DIR" ]; then
  die "未找到 build-tools（签名需要 apksigner）。请确认 ANDROID_HOME 下已安装 build-tools。"
fi
APKSIGNER="$BT_DIR/apksigner.bat"
KS="${PMS_KEYSTORE:-D:/AndroidDev/keystores/passwordms-release.jks}"
KS_ALIAS="${PMS_KEY_ALIAS:-passwordms}"
PASS_FILE="${PMS_KEY_PASS_FILE:-D:/AndroidDev/keystores/release.pass}"
if [ -n "${PMS_KEY_PASS:-}" ]; then
  KS_PASS="$PMS_KEY_PASS"
elif [ -f "$PASS_FILE" ]; then
  KS_PASS="$(tr -d '\r\n' < "$PASS_FILE")"
else
  die "未找到签名口令：请创建 $PASS_FILE（单行 keystore 口令），或用 PMS_KEY_PASS / PMS_KEY_PASS_FILE 传入"
fi
if [ ! -f "$KS" ]; then
  die "签名 keystore 不存在：$KS（可用环境变量 PMS_KEYSTORE 指定其他 keystore）"
fi
APP_VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
SIGNED_APK="target/PasswordMS-${APP_VERSION}-arm64.apk"
"$APKSIGNER" sign --ks "$KS" --ks-key-alias "$KS_ALIAS" \
  --ks-pass "pass:$KS_PASS" --key-pass "pass:$KS_PASS" \
  --out "$SIGNED_APK" "$APK" || die "apksigner sign 失败"
"$APKSIGNER" verify "$SIGNED_APK" || die "签名后验证失败：$SIGNED_APK"
"$BT_DIR/zipalign.exe" -c 4 "$SIGNED_APK" || die "zipalign 对齐校验失败：$SIGNED_APK"
ok "签名 + 验证 + 对齐校验通过：$SIGNED_APK"
APK="$SIGNED_APK"

# ---------- 8. 体积核验 ----------
SIZE_BYTES="$(wc -c < "$APK" | tr -d '[:space:]')"
SIZE_MB="$(awk "BEGIN { printf \"%.2f\", $SIZE_BYTES/1048576 }")"
log "APK：$APK（${SIZE_MB} MB / ${SIZE_BYTES} 字节）"
awk "BEGIN { exit !($SIZE_MB <= $APK_LIMIT_MB) }" || die "APK 体积 ${SIZE_MB} MB 超出上限 ${APK_LIMIT_MB} MB"
ok "体积核验通过（≤ ${APK_LIMIT_MB} MB）。"

# ---------- 9. 成功收尾 ----------
log "构建完成：$APK（${SIZE_MB} MB）；包内 .so 已通过四条断言校验，已签名（apksigner V3）并通过 zipalign 对齐校验。"

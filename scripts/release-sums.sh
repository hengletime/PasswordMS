#!/usr/bin/env bash
# 生成发布用的 SHA256SUMS-<version>.txt（四件产物，四行，无空行）。
#
# 为什么单独一支脚本：1.5.29 之后这块被人肉省掉了 7 个版本，Release 页上
# 直接少了这个文件——用户下载后没法独立核对字节。发布完整性不该依赖记忆。
#
# 用法：bash scripts/release-sums.sh [版本号]
#       省略版本号则从 src-tauri/tauri.conf.json 读当前版本。
# 产出：release/SHA256SUMS-<version>.txt，写完后回读自检 4/4。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REL="target/x86_64-pc-windows-gnullvm/release"
VERSION="${1:-$(grep -oP '(?<="version": ")[^"]+' src-tauri/tauri.conf.json | head -1)}"
OUT="release/SHA256SUMS-${VERSION}.txt"

FILES=(
  "$REL/bundle/nsis/PasswordMS_${VERSION}_x64-setup.exe"
  "$REL/bundle/msi/PasswordMS_${VERSION}_x64_en-US.msi"
  "$REL/PasswordMS-${VERSION}-portable-x64.zip"
  "target/PasswordMS-${VERSION}-arm64.apk"
)

missing=0
for f in "${FILES[@]}"; do
  [ -f "$f" ] || { echo "FATAL: 缺产物 $f" >&2; missing=1; }
done
[ "$missing" -eq 0 ] || { echo "FATAL: 四件产物不齐，不能生成校验清单" >&2; exit 1; }

mkdir -p release
: > "$OUT"
for f in "${FILES[@]}"; do
  printf '%s  %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$(basename "$f")" >> "$OUT"
done

# 回读自检：清单里的每一行都要能对回磁盘（防止写串行/写漏行）
echo "== $OUT =="
cat "$OUT"
ok=0
for f in "${FILES[@]}"; do
  name="$(basename "$f")"
  want="$(awk -v n="$name" '$2 == n { print $1 }' "$OUT")"
  got="$(sha256sum "$f" | cut -d' ' -f1)"
  if [ -n "$want" ] && [ "$want" = "$got" ]; then
    ok=$((ok + 1))
  else
    echo "FAIL: $name 清单里读不到或对不上（清单 '$want' / 磁盘 '$got'）" >&2
    exit 1
  fi
done
echo "校验清单自检 OK：$ok/4"

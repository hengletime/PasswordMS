#!/usr/bin/env bash
# 打包「便携绿色版」zip（Windows）。
#
# 产物：PasswordMS-<version>-portable-x64.zip，内含
#   passwordms.exe            主程序（前端/Rust 核心/图标全部内嵌）
#   WebView2Loader.dll        WebView2 加载库（运行时依赖系统 WebView2，Win11 自带）
#   passwordms.portable       便携模式标记（空文件）：存在即把数据文件落在 exe 旁
#   使用说明.txt
#
# 用法：bash scripts/make-portable.sh
# 前置：先完成 `npx tauri build --target x86_64-pc-windows-gnullvm`（产物在
#       target/x86_64-pc-windows-gnullvm/release/）。
set -euo pipefail

REL="target/x86_64-pc-windows-gnullvm/release"
VERSION=$(grep -oP '(?<="version": ")[^"]+' src-tauri/tauri.conf.json | head -1)
OUT="$REL/PasswordMS-${VERSION}-portable-x64.zip"
STAGE="$TEMP/pms-portable-stage"

[ -f "$REL/passwordms.exe" ] || { echo "FATAL: 先跑 npx tauri build --target x86_64-pc-windows-gnullvm"; exit 1; }
[ -f "$REL/WebView2Loader.dll" ] || { echo "FATAL: 找不到 WebView2Loader.dll"; exit 1; }

rm -rf "$STAGE" && mkdir -p "$STAGE"
cp "$REL/passwordms.exe" "$STAGE/"
cp "$REL/WebView2Loader.dll" "$STAGE/"
: > "$STAGE/passwordms.portable"
cat > "$STAGE/使用说明.txt" << 'EOF'
PasswordMS 便携绿色版
=====================

【这是什么】
免安装版密码管理器：解压即可运行，全部数据随文件夹走（U 盘、移动硬盘均可）。

【怎么用】
1. 解压到任意可写目录（不要直接在压缩软件预览窗里运行）。
2. 双击 passwordms.exe。首次运行会引导你创建保险库。
3. 数据文件（passwordms.vault / passwordms.identity）自动生成在本目录。
   想搬走全部数据：退出程序后把整个文件夹拷走即可。

【升级】
用新版 passwordms.exe 和 WebView2Loader.dll 覆盖旧文件即可；
passwordms.portable 标记与数据文件保持不动。

【改名 / 搬移文件夹】
1. 先彻底退出程序：右下角托盘图标右键 → 退出。
   （点窗口右上角的 × 只是收进托盘，程序仍在运行。）
2. 整个文件夹一起搬，这几样缺一不可：
   passwordms.exe、WebView2Loader.dll、passwordms.portable、
   passwordms.vault、passwordms.identity。
   少一个 passwordms.portable，程序就会去另一个目录找库。
3. 搬完请检查桌面和开始菜单：如果还留着旧的 PasswordMS 图标，删掉它。
   旧图标可能指向另一份安装版，打开的是完全不同的另一个库 ——
   两个界面一模一样，你只会以为"密码不对、指纹也不对"。
   程序现在会在解锁页底部显示当前数据目录，对照一下就知道打开的是哪一份。

【安全提示】
- 保险库为整卷加密（Argon2id + XChaCha20-Poly1305），但没有主密码谁也解不开 ——
  请务必牢记主密码，忘了没有任何找回手段。
- 建议 12 位以上混合主密码；重要数据请定期用「设置 → 导出」做备份。
- 从 U 盘拔出前请先退出程序（右上角关闭按钮），避免数据尚未写完。

【WebView2 说明】
本程序依赖微软 WebView2 运行时（Windows 10 1803+ / Windows 11 一般已自带）。
若启动报错提示缺 WebView2，到微软官网搜索 "WebView2 Runtime" 下载 Evergreen 安装版。
EOF

PY="/c/Users/HUAWEI/.workbuddy/binaries/python/versions/3.13.12/python.exe"
"$PY" - "$STAGE" "$OUT" << 'PYEOF'
import sys, zipfile, pathlib
stage, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
files = sorted(p.name for p in stage.iterdir())
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for name in files:
        z.write(stage / name, f'PasswordMS-Portable/{name}')
print(f'[portable] {out} ({out.stat().st_size:,} B) files={files}')
assert len(files) == 4, 'expected exactly 4 files in stage'
PYEOF

# 产物校验：zip 可读且含标记文件
"$PY" - "$OUT" << 'PYEOF'
import sys, zipfile
names = zipfile.ZipFile(sys.argv[1]).namelist()
assert any(n.endswith('passwordms.portable') for n in names), 'marker missing'
assert any(n.endswith('passwordms.exe') for n in names), 'exe missing'
assert any(n.endswith('WebView2Loader.dll') for n in names), 'dll missing'
print('[portable] integrity check OK:', len(names), 'entries')
PYEOF
echo "[portable] done: $OUT"

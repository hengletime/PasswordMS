#!/usr/bin/env python3
"""把一版产物发布到 GitHub Releases —— 带前置校验、幂等、终核。

为什么要有这支脚本（2026-11-04 教训）：
1.5.29 ~ 1.5.35 之间，Release 页的「名称 / 正文语言 / 校验清单」三样全部漂了：
名称退化成裸 tag（`v1.5.33`，产品名没了）、正文变成从 commit 自动生成的英文
`## Changed`、`SHA256SUMS-<版本>.txt` 干脆消失。根因不是不会做，是**发布靠记忆**
——手动调 API 时少写一个字段没人拦。所以把「一版发布的全部要件」写成硬校验，
缺一条就拒绝发，宁可发不出去也不要发一张残缺的页面。

用法：
    python scripts/publish-release.py 1.5.36                 # 正常发布（创建或更新）
    python scripts/publish-release.py 1.5.36 --dry-run       # 只跑校验，不动线上
    python scripts/publish-release.py 1.5.34 --historical    # 补历史版本的说明与校验清单
    python scripts/publish-release.py 1.5.32 --historical --name "PasswordMS 1.5.32 — …" \\
        --keep-body --skip-upload                            # 只订正旧页面的名称

前置（缺一不可，脚本会逐条查）：
  · 三个版本号一致（package.json / tauri.conf.json / Cargo.toml workspace）
  · docs/releases/v<版本号>.md 存在，且满足发布说明六条硬要求
  · 四件产物 + release/SHA256SUMS-<版本号>.txt 存在、体积在红线内、清单能对上
  · target/.gh_cred 里有可用令牌（git credential fill 写出，用完删）

发布后终核：线上 5 个资产全部 uploaded，且每个的 digest 与本地 sha256 逐字相等。
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)

API = "https://api.github.com"
REPO = "hengletime/PasswordMS"

REL = "target/x86_64-pc-windows-gnullvm/release"
MB10 = 10 * 1024 * 1024
MB12 = 12 * 1024 * 1024

# 发布说明六条硬要求（与 docs/releases/_TEMPLATE.md 注释一一对应）
REQUIRED_SECTIONS = ("## 已知限制", "## 交付信息", "## English summary")
MIN_CJK = 300

TOKEN = ""


def die(msg: str) -> None:
    print("FATAL: " + msg, file=sys.stderr)
    raise SystemExit(1)


def ok(msg: str) -> None:
    print("\033[1;32m[ OK ]\033[0m " + msg, flush=True)


def info(msg: str) -> None:
    print("\033[1;34m[ .. ]\033[0m " + msg, flush=True)


# ---------------------------------------------------------------- 令牌与 HTTP


def read_token() -> str:
    path = "target/.gh_cred"
    if not os.path.exists(path):
        die(
            "缺 %s。先执行（Git Bash）：\n"
            '  printf "protocol=https\\nhost=github.com\\n" | git credential fill > target/.gh_cred\n'
            "发布完立刻 rm 掉该文件。" % path
        )
    token = ""
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            if line.startswith("password="):
                token = line.split("=", 1)[1].strip()
    if not token:
        die("%s 里没有 password= 行，重新用 git credential fill 生成" % path)
    return token


def make_openers():
    """两条路都准备好：直连 + 环境变量里的本地代理。

    这台机器上 GitHub 的可达性会来回翻（2026-11-04 同一小时内实测过三种状态：
    直连 SSL EOF / 代理 502 / curl 走代理通但 python 走代理握手失败）。
    所以不猜哪条通，失败一次就换另一条，谁先成功就一直用它。
    """
    direct = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    proxies = {}
    for scheme in ("http", "https"):
        v = os.environ.get(scheme + "_proxy") or os.environ.get(scheme.upper() + "_PROXY")
        if v:
            proxies[scheme] = v
    proxied = urllib.request.build_opener(urllib.request.ProxyHandler(proxies)) if proxies else None
    return direct, proxied


DIRECT, PROXIED = make_openers()
ROUTE = "direct"


def _opener():
    if ROUTE == "proxy" and PROXIED is not None:
        return PROXIED
    return DIRECT


def call(method: str, url: str, payload=None, timeout: int = 60, tries: int = 10, allow_404: bool = False):
    """带退避重试与路线切换的 API 调用。"""
    global ROUTE
    data = None
    headers = {
        "Authorization": "Bearer " + TOKEN,
        "Accept": "application/vnd.github+json",
        "User-Agent": "passwordms-release",
    }
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    last = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, data=data, headers=headers, method=method)
            with _opener().open(req, timeout=timeout) as r:
                body = r.read()
                return json.loads(body.decode("utf-8")) if body else None
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:400]
            if allow_404 and e.code == 404:
                return None
            if e.code in (401, 403, 404, 422):
                die("%s %s -> HTTP %d %s" % (method, url, e.code, detail))
            last = "HTTP %d %s" % (e.code, detail)
        except Exception as e:  # noqa: BLE001
            last = str(e)
        print("RETRY %d/%d [%s] %s %s: %s" % (i + 1, tries, ROUTE, method, url.rsplit("/", 1)[-1], last), flush=True)
        # 换一条路再试：直连 <-> 代理
        if PROXIED is not None:
            ROUTE = "proxy" if ROUTE == "direct" else "direct"
        time.sleep(6)
    die("%s %s 连续 %d 次失败（直连与代理都试过）：%s" % (method, url, tries, last))


def upload(upload_url: str, path: str, tries: int = 8):
    global ROUTE
    name = os.path.basename(path)
    url = upload_url.split("{")[0] + "?name=" + urllib.parse.quote(name)
    with open(path, "rb") as f:
        blob = f.read()
    headers = {
        "Authorization": "Bearer " + TOKEN,
        "Accept": "application/vnd.github+json",
        "Content-Type": "application/octet-stream",
        "User-Agent": "passwordms-release",
    }
    last = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, data=blob, headers=headers, method="POST")
            with _opener().open(req, timeout=300) as r:
                return json.loads(r.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:300]
            die("上传 %s -> HTTP %d %s" % (name, e.code, detail))
        except Exception as e:  # noqa: BLE001
            last = str(e)
        print("RETRY %d/%d [%s] upload %s: %s" % (i + 1, tries, ROUTE, name, last), flush=True)
        if PROXIED is not None:
            ROUTE = "proxy" if ROUTE == "direct" else "direct"
        time.sleep(8)
    die("上传 %s 连续 %d 次失败：%s" % (name, tries, last))


def sha256_of(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ---------------------------------------------------------------- 本地前置校验


def check_versions(version: str) -> None:
    with open("package.json", "r", encoding="utf-8") as f:
        pkg = json.load(f)["version"]
    with open("src-tauri/tauri.conf.json", "r", encoding="utf-8") as f:
        tauri = json.load(f)["version"]
    with open("Cargo.toml", "r", encoding="utf-8") as f:
        m = re.search(r'\[workspace\.package\][^\[]*?version\s*=\s*"([^"]+)"', f.read(), re.S)
    cargo = m.group(1) if m else "?"
    triple = (("package.json", pkg), ("tauri.conf.json", tauri), ("Cargo.toml", cargo))
    if any(v != version for _, v in triple):
        die("版本号不一致：%s —— 先统一四处版本（含 Cargo.lock，跑 bash scripts/check-version.sh）"
            % ", ".join("%s=%s" % (n, v) for n, v in triple))
    ok("版本号三处一致：%s" % version)


def check_notes(version: str) -> str:
    path = "docs/releases/v%s.md" % version
    if not os.path.exists(path):
        die(
            "缺发布说明 %s。\n"
            "从 docs/releases/_TEMPLATE.md 复制一份再填。这一版的说明必须进版本库，\n"
            "不能只贴在 GitHub 正文里——1.5.30~1.5.35 的说明就是这么丢的。" % path
        )
    with open(path, "r", encoding="utf-8") as f:
        body = f.read().strip()

    first = body.splitlines()[0].strip()
    if first != "# PasswordMS " + version:
        die("%s 首行必须是 `# PasswordMS %s`，当前是：%s" % (path, version, first))
    ok("说明标题：%s" % first)

    cjk = len(re.findall(r"[\u4e00-\u9fff]", body))
    if cjk < MIN_CJK:
        die("%s 只有 %d 个汉字（下限 %d）；从 commit 机翻出来的英文正文不算数。" % (path, cjk, MIN_CJK))
    ok("中文正文 %d 字（下限 %d）" % (cjk, MIN_CJK))

    missing = [s for s in REQUIRED_SECTIONS if s not in body]
    if missing:
        die("%s 缺小节：%s（没有内容就写「无」，不许省略）" % (path, "、".join(missing)))
    ok("必备小节齐全")

    if "## Changed" in body:
        die("%s 里还有自动生成的 `## Changed` 小标题，改成用户能看懂的说法" % path)

    # 首行 H1 只用于版本库内检索：页面上标题已经显示同一个名字，
    # 正文里再来一次是重复的两行同名大字，所以发出去时去掉。
    lines = body.splitlines()
    return "\n".join(lines[1:]).lstrip("\n")


def artifact_paths(version: str):
    return [
        ("%s/bundle/nsis/PasswordMS_%s_x64-setup.exe" % (REL, version), MB10),
        ("%s/bundle/msi/PasswordMS_%s_x64_en-US.msi" % (REL, version), MB10),
        ("%s/PasswordMS-%s-portable-x64.zip" % (REL, version), MB10),
        ("target/PasswordMS-%s-arm64.apk" % version, MB12),
        ("release/SHA256SUMS-%s.txt" % version, 64 * 1024),
    ]


def check_artifacts(version: str):
    paths = []
    for path, limit in artifact_paths(version):
        if not os.path.exists(path):
            hint = "\n  生成：bash scripts/release-sums.sh %s" % version if path.endswith(".txt") else ""
            die("缺产物 %s%s" % (path, hint))
        size = os.path.getsize(path)
        if size == 0:
            die("%s 是 0 字节" % path)
        if size > limit:
            die("%s 体积 %d 超红线 %d" % (os.path.basename(path), size, limit))
        paths.append(path)
    ok("五件齐备：" + ", ".join(os.path.basename(p) for p in paths))

    sums_path = paths[-1]
    with open(sums_path, "r", encoding="utf-8") as f:
        lines = [ln for ln in f.read().splitlines() if ln.strip()]
    if len(lines) != 4:
        die("%s 应为 4 行，实际 %d 行" % (sums_path, len(lines)))
    table = {}
    for ln in lines:
        digest, name = ln.split("  ", 1)
        table[name.strip()] = digest.strip()
    for p in paths[:-1]:
        name = os.path.basename(p)
        if name not in table:
            die("%s 未列入校验清单" % name)
        actual = sha256_of(p)
        if actual != table[name]:
            die("%s 与校验清单不符（磁盘 %s）—— 重跑 bash scripts/release-sums.sh %s" % (name, actual, version))
    ok("校验清单与磁盘逐件一致：4/4")
    return paths


# ---------------------------------------------------------------- 主流程


def main() -> None:
    global TOKEN

    args = sys.argv[1:]
    if not args or args[0].startswith("-"):
        die("用法：python scripts/publish-release.py <版本号> "
            "[--dry-run] [--historical] [--name 文本] [--keep-body] [--skip-upload]")
    version = args[0].lstrip("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        die("版本号格式应为 X.Y.Z，收到：%s" % version)

    dry = "--dry-run" in args
    historical = "--historical" in args
    keep_body = "--keep-body" in args
    skip_upload = "--skip-upload" in args
    name = "PasswordMS " + version
    if "--name" in args:
        name = args[args.index("--name") + 1]

    print("== 前置校验 ==")
    if historical:
        info("--historical：跳过「源文件版本相符」检查（补历史页面用）")
    else:
        check_versions(version)

    body = None
    if keep_body:
        info("--keep-body：保留线上正文")
    else:
        body = check_notes(version)
    strip_h1 = "--strip-body-h1" in args
    if strip_h1:
        if not keep_body:
            die("--strip-body-h1 只能配合 --keep-body 使用（它作用于线上已有正文）")
        info("--strip-body-h1：去掉正文开头那行与标题重复的 H1")

    paths = []
    if not skip_upload:
        paths = check_artifacts(version)

    print("\n== 发布内容 ==")
    print("tag   : v%s" % version)
    print("name  : %s" % name)
    print("body  : %s" % ("(保留线上正文)" if keep_body else "%d 字符" % len(body)))
    print("资产  : %s" % ("(跳过上传)" if skip_upload else ", ".join(os.path.basename(p) for p in paths)))

    if dry:
        print("\n--dry-run：校验通过，未改动线上")
        return

    TOKEN = read_token()
    tag = "v" + version
    print("\n== 同步线上 ==")

    existing = call("GET", "%s/repos/%s/releases/tags/%s" % (API, REPO, tag), allow_404=True)

    payload = {"name": name}
    if body is not None:
        payload["body"] = body
    if strip_h1 and existing and existing.get("body"):
        lines = existing["body"].lstrip("\n").splitlines()
        if lines and lines[0].startswith("# ") and "PasswordMS" in lines[0]:
            payload["body"] = "\n".join(lines[1:]).lstrip("\n")
            info("正文首行已去掉：%s" % lines[0])

    if existing is None:
        if body is None:
            die("线上没有 %s，且 --keep-body 没有正文可保留" % tag)
        payload.update({"tag_name": tag, "draft": False, "prerelease": False})
        rel = call("POST", "%s/repos/%s/releases" % (API, REPO), payload)
        ok("已创建 release id=%s" % rel["id"])
    else:
        rel = call("PATCH", "%s/repos/%s/releases/%s" % (API, REPO, existing["id"]), payload)
        ok("已更新 release id=%s（名称/正文；原有资产保留）" % rel["id"])

    if not skip_upload:
        have = {a["name"]: a for a in rel.get("assets") or []}
        for path in paths:
            fname = os.path.basename(path)
            digest = "sha256:" + sha256_of(path)
            size = os.path.getsize(path)
            cur = have.get(fname)
            if cur and cur.get("state") == "uploaded" and cur.get("size") == size and cur.get("digest") == digest:
                ok("线上已有一致资产：%s" % fname)
                continue
            if cur:
                info("同名资产不一致，先删再传：%s" % fname)
                call("DELETE", "%s/repos/%s/releases/assets/%s" % (API, REPO, cur["id"]))
            info("上传 %s（%d 字节）" % (fname, size))
            up = upload(rel["upload_url"], path)
            if up.get("size") != size:
                die("上传后尺寸不符：%s 线上 %s / 本地 %d" % (fname, up.get("size"), size))
            ok("已上传 %s state=%s" % (fname, up.get("state")))

    print("\n== 终核 ==")
    final = call("GET", "%s/repos/%s/releases/%s" % (API, REPO, rel["id"]))
    if final.get("name") != name:
        die("名称没写进去：线上仍是 %r" % final.get("name"))
    ok("名称：%s" % final["name"])

    assets = {a["name"]: a for a in final.get("assets") or []}
    if paths:
        for path in paths:
            fname = os.path.basename(path)
            if fname not in assets:
                die("线上缺资产：%s" % fname)
            if assets[fname].get("digest") != "sha256:" + sha256_of(path):
                die("digest 不符：%s 线上 %s" % (fname, assets[fname].get("digest")))
        ok("五个资产 digest 全部与本地一致")
    print("\n完成：https://github.com/%s/releases/tag/%s" % (REPO, tag))


if __name__ == "__main__":
    main()

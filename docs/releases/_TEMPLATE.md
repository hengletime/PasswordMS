# PasswordMS <版本号>

<!--
发布说明模板。每次发版复制成 docs/releases/v<版本号>.md，然后把尖括号占位全填掉。
硬要求（docs/07-quality-gate.md §5 会逐条检查，缺一条发布脚本直接拒绝）：
  1. 标题必须是 `# PasswordMS <版本号>`（产品名不能少）
  2. 第一段用一句人话说清「这版到底改了什么」，不写「若干优化」这种空话
  3. 中文正文 ≥ 300 个汉字（自动生成的英文 ## Changed 会被拒）
  4. 必须有 `## 已知限制` 一节——没有就写「无」，不许省略
  5. 必须有 `## 交付信息` 表，四件产物的字节数与 sha256 与资产逐字一致
  6. 末尾附 `## English summary` 一段（非中文用户能看懂这版干了什么）
-->

这一版最要紧的一件事：<一句话，讲用户能感知到的结果，不讲实现>。

## <主标题：用用户的语言描述问题，不用缺陷编号当标题>

<先说现象：用户看到了什么、后果是什么；再说现在怎么判/怎么挡；最后说有没有让步。>

## <次标题>

<同上。>

## 已知限制

- <做不到的部分，或需要用户注意的前提。没有就写「无」。>

## 交付信息

| 产物 | 字节 | sha256 |
|---|---|---|
| `PasswordMS_<版本号>_x64-setup.exe` |  |  |
| `PasswordMS_<版本号>_x64_en-US.msi` |  |  |
| `PasswordMS-<版本号>-portable-x64.zip` |  |  |
| `PasswordMS-<版本号>-arm64.apk` |  |  |

同一组数值也在资产 `SHA256SUMS-<版本号>.txt` 里，可用 `sha256sum -c SHA256SUMS-<版本号>.txt` 逐件核对。

**绿色版请用 `PasswordMS-<版本号>-portable-x64.zip`**，不要只拷裸 exe——裸 exe 缺 WebView2 引导与图标资源。

## English summary

<One short paragraph: what changed for the user, in plain English.>

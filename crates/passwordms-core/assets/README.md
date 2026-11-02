# 第三方资源

## eff_large.wordlist

- **用途**：口令短语生成（v1.5.29 / B-04）。
- **来源**：EFF 官方的 long wordlist —
  <https://www.eff.org/files/2016/07/18/eff_large_wordlist.txt>，
  本仓库中的副本取自 KeePassXC 2.8.0 发行包 `share/wordlists/eff_large.wordlist`
  （`Reference/KeePassXC-2.8.0-x64/keepassxc-2.8.0/share/wordlists/`）。
- **许可**：Creative Commons Attribution 3.0 US（CC BY 3.0 US）。
  要求保留署名：EFF 与「EFF's New Wordlists for Random Passphrases」一文。
  应用「关于」页与应用内许可文本须列出该项。
- **条数与熵**：本副本共 **7772 条**，无重复、无空行，每词熵 `log2(7772) ≈ 12.9241 bits`。
  注意：EFF 官方文档宣称 7776 条，与这份实际内容不一致——因此代码里
  **熵必须按运行时读到的条数计算**，不得硬编码 7776 或 12.9。
  `crates/passwordms-core/src/passphrase.rs` 的守门测试会同时钉住条数与 sha256，
  词表被替换或截断都会红。
- **校验**：`sha256(df895130803573b14caa684e8cba982a9ed21f4898c98c711f204acf600efedd)`，
  62,122 字节。

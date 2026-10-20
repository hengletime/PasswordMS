# PasswordMS

**Zero-knowledge, cross-platform password manager** — Windows desktop + Android, with end-to-end encrypted LAN sync.

English | [简体中文](README.zh-CN.md)

[Download latest release](https://github.com/hengletime/PasswordMS/releases/latest) · [Report an issue](https://github.com/hengletime/PasswordMS/issues)

---

## ✨ Features

- **Zero-knowledge encryption**: whole-vault encryption (Argon2id key derivation + XChaCha20-Poly1305 authenticated encryption). The master password is never uploaded and cannot be recovered.
- **Two platforms**: Windows 10/11 (installer / portable version) + Android (arm64)
- **LAN sync**: QR-code pairing, end-to-end encrypted — data never touches any cloud server
- **Auto-lock**: locks automatically after inactivity (1 / 5 / 15 minutes); Android locks immediately when returning from background after timeout
- **Attachments**: login / note entries can carry attachments (up to 16 per entry, ≤ 2 MB each), encrypted with the vault
- **Recycle bin**: soft delete → restore → permanent purge; attachments are cleaned up alongside entries
- **Tiny footprint**: desktop installer ≈ 1.6 MB, APK ≈ 6.9 MB

## 📦 Download

Grab the latest build from [Releases](https://github.com/hengletime/PasswordMS/releases/latest):

| File | Platform | Notes |
|---|---|---|
| `PasswordMS_x.y.z_x64-setup.exe` | Windows 10/11 x64 | Installer (recommended) |
| `PasswordMS_x.y.z_x64_en-US.msi` | Windows 10/11 x64 | MSI installer (enterprise deployment) |
| `PasswordMS_x.y.z-portable-x64.zip` | Windows 10/11 x64 | **Portable**: unzip & run, data stays in the folder (USB-stick friendly) |
| `PasswordMS-x.y.z-arm64.apk` | Android 8.0+ (arm64) | Sideload (allow installing from unknown sources) |

> Portable notes: unzip into any **writable directory** (do not run from inside the archiver's preview window), then double-click `passwordms.exe`. Data files are created next to the exe. To upgrade, overwrite the old `passwordms.exe` and `WebView2Loader.dll` with the new ones.

## 🚀 Quick Start

1. Download and install (or unzip the portable version)
2. Create a vault on first launch and set a **master password** — memorize it: **there is no recovery mechanism whatsoever**
3. Add login / note entries; drag in attachments as needed
4. Multi-device sync: `Devices → Pair new device`, then scan the QR code from the other device (both devices must be on the same LAN)

## 🔐 Security Model

- The vault is **fully encrypted**; keys are derived from the master password via Argon2id and never written to disk
- LAN sync is end-to-end encrypted: paired devices share the vault key; traffic is unreadable to other devices on the network
- Auto-lock after inactivity; sensitive data in memory is zeroized when locked
- This software **never uploads any data** — everything stays on your machine (and the LAN devices you explicitly pair)

## ⚠️ Disclaimer / Terms of Use

1. **No master password recovery**: this software has no backdoor and offers no escrow. Forgetting the master password means the data is permanently unrecoverable. Keep it safe and back up regularly via `Settings → Export`.
2. **Provided "as is"**: despite thorough testing, the software is not guaranteed defect-free. Use at your own risk and **back up important data regularly**.
3. **Not professional security advice**: this software does not replace a full information-security strategy; use it on trusted devices and keep your OS and WebView2 runtime up to date.
4. **LAN risks**: paired devices share encrypted data. Do not pair over untrusted networks (e.g. public Wi-Fi).
5. By using this software you acknowledge and agree to the terms above. The software is released under the [MIT License](LICENSE), which applies alongside these terms.

## 🛠️ Building from Source

Stack: Tauri 2 + Svelte 5 (runes) + Vite 6 + TypeScript 5 + Rust workspace.

```bash
npm install                      # install frontend dependencies
npm run build                    # frontend build (outputs to dist/)
npx tauri build --target x86_64-pc-windows-gnullvm   # Windows desktop (installer + portable exe)
bash scripts/make-portable.sh    # package the portable zip
bash scripts/build-android.sh    # Android APK
```

## 📄 License

[MIT](LICENSE)

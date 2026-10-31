# PasswordMS

**Zero-knowledge, cross-platform password manager** — Windows desktop + Android, with end-to-end encrypted LAN sync.

English | [简体中文](README.zh-CN.md)

[Download latest release](https://github.com/hengletime/PasswordMS/releases/latest) · [Report an issue](https://github.com/hengletime/PasswordMS/issues)

---

## ✨ Features

- **Zero-knowledge encryption**: whole-vault encryption (Argon2id key derivation + XChaCha20-Poly1305 authenticated encryption). The master password is never uploaded and cannot be recovered.
- **Two platforms**: Windows 10/11 (installer / portable version) + Android (arm64)
- **LAN sync**: QR-code pairing, end-to-end encrypted — data never touches any cloud server (manual trigger; see "How sync works" below)
- **Auto-lock**: locks automatically after inactivity (1 / 5 / 15 minutes); Android locks immediately when returning from background after timeout
- **Attachments**: login / note entries can carry attachments (up to 16 per entry, ≤ 20 MB each), encrypted with the vault
- **Trash**: soft delete → restore → delete permanently; attachments are cleaned up alongside entries
- **Tiny footprint**: desktop installer ≈ 2.0 MB, APK ≈ 7.4 MB

## 📦 Download

Grab the latest build from [Releases](https://github.com/hengletime/PasswordMS/releases/latest):

| File | Platform | Notes |
|---|---|---|
| `PasswordMS_x.y.z_x64-setup.exe` | Windows 10/11 x64 | Installer (recommended) |
| `PasswordMS_x.y.z_x64_en-US.msi` | Windows 10/11 x64 | MSI installer (enterprise deployment) |
| `PasswordMS_x.y.z-portable-x64.zip` | Windows 10/11 x64 | **Portable**: unzip & run, data stays in the folder (USB-stick friendly) |
| `PasswordMS-x.y.z-arm64.apk` | Android 7.0+ (arm64) | Sideload (allow installing from unknown sources) |

> Portable notes: unzip into any **writable directory** (do not run from inside the archiver's preview window), then double-click `passwordms.exe`. Data files are created next to the exe. To upgrade, overwrite the old `passwordms.exe` and `WebView2Loader.dll` with the new ones.

## 🚀 Quick Start

1. Download and install (or unzip the portable version)
2. Create a vault on first launch and set a **master password** — memorize it: **there is no recovery mechanism whatsoever**
3. Add login / note entries; drag in attachments as needed
4. Multi-device sync: `Devices → Pair new device`, then scan the QR code from the other device (both devices must be on the same LAN). Sync is **manual** — either side can trigger it; see the next section

## 🔄 How sync works (one hard limitation)

**There is no background auto-sync.** Sync happens only when you tap "Sync now". This is a deliberate trade-off: once a phone's screen turns off, Android silently drops that app's network traffic. That claim is backed by a measured comparison — while the phone is **awake**, a connection attempt to a port nobody listens on comes back with an immediate `RST` (the network works and the kernel refuses properly); once the phone is **asleep**, a sweep of 16,405 ports gets zero replies, not even `RST` (`ping` still works, because ICMP is not subject to per-app network restrictions). Keeping the device always reachable would require a resident foreground service (permanent notification + battery cost), which is not worth it.

**Either side can start a sync, but the peer must be awake:**

| Where you tap "Sync now" | Result |
|---|---|
| Desktop → phone | Only works while the phone is **awake with PasswordMS in the foreground**; a locked or sleeping phone receives nothing at all |
| Phone → desktop | The desktop is normally always awake, so this direction is **far more reliable** |

So if "Sync now" on the desktop does nothing, **pick up the phone, turn the screen on, open PasswordMS and tap "Sync now" in the sync page** — just reverse the direction.

Other requirements and notes:

- Both devices must be on the **same Wi‑Fi** (same subnet)
- After switching networks or IPs, the address stored at pairing becomes stale; one sync attempt performs LAN discovery and updates it (the new address is only saved after a successful sync)
- If the two sides keep showing different addresses, or discovery never gets a reply: **re-scan the pairing QR code** to recover immediately
- **On Android the app needs the "Nearby devices" permission**: since Android 17 (API 37), LAN access requires the `ACCESS_LOCAL_NETWORK` runtime grant. Without it the system **silently drops** this app's connections (you see "network timeout" while the same phone's browser and keyboard still reach the LAN just fine). From v1.5.0 the app asks for it on launch — **tap Allow**. If you declined, re-enable it under Settings → Apps → PasswordMS → Permissions → Nearby devices
- **Last-resort fallback**: enable a **mobile hotspot** on the desktop and connect the phone to it before pairing/syncing — inside a hotspot the desktop *is* the phone's gateway, and that path is not affected by the permission above (measured 2026-10-30 on two routers' Wi‑Fi vs hotspot; root-caused 2026-10-31 to the missing permission, fixed in v1.5.0)
- The sync page shows this device's IP, the current listening port, and the recorded address of every paired device — compare both ends to tell "stale address" apart from "peer is away"

## 🔐 Security Model

- The vault is **fully encrypted**; keys are derived from the master password via Argon2id and never written to disk
- LAN sync is end-to-end encrypted: paired devices share the vault key; traffic is unreadable to other devices on the network
- Auto-lock after inactivity; sensitive data in memory is zeroized when locked
- This software **never uploads any data** — everything stays on your machine (and the LAN devices you explicitly pair)

## ⚠️ Disclaimer / Terms of Use

1. **No master password recovery**: this software has no backdoor and offers no escrow. Forgetting the master password means the data is permanently unrecoverable. Keep it safe and back up regularly via `Settings → Export`.
2. **Uninstalling takes the vault with it**: the vault is never backed up for you — on Android, `allowBackup` is deliberately **disabled** so the vault never reaches a third-party cloud. Deleting the app, switching devices, or reinstalling the OS therefore deletes the local vault file. **Export an encrypted copy first** via `Settings → Export vault` (restore it from the lock screen with "Restore from backup"). When uninstalling from system Settings, Android asks whether to keep app data — tick that box and reinstalling an APK with the same signature restores it in place; uninstalling from an app store or `adb uninstall` shows no such prompt.
3. **Check the exported file size yourself**: Settings keeps showing "Last successful export: <time> (<size>)", and the success toast now states the size. **A 0 B backup is no backup at all** — that was exactly the Android export failure before 1.5.2, now fixed. The export is an encrypted copy: whoever holds it still needs the master password, but keep it somewhere safe.
4. **Provided "as is"**: despite thorough testing, the software is not guaranteed defect-free. Use at your own risk and **back up important data regularly**.
5. **Not professional security advice**: this software does not replace a full information-security strategy; use it on trusted devices and keep your OS and WebView2 runtime up to date.
6. **LAN risks**: paired devices share encrypted data. Do not pair over untrusted networks (e.g. public Wi-Fi).
7. By using this software you acknowledge and agree to the terms above. The software is released under the [MIT License](LICENSE), which applies alongside these terms.

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

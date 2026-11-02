# PasswordMS

**Zero-knowledge, cross-platform password manager** — Windows desktop + Android, with end-to-end encrypted LAN sync.

English | [简体中文](README.zh-CN.md)

[Download latest release](https://github.com/hengletime/PasswordMS/releases/latest) · [Report an issue](https://github.com/hengletime/PasswordMS/issues)

---

## ✨ Features

**On the security side**

- **Zero-knowledge encryption**: whole-vault encryption (Argon2id key derivation + XChaCha20-Poly1305 authenticated encryption). The master password is never uploaded and cannot be recovered.
- **Auto-lock**: locks after 1 / 5 / 15 minutes of inactivity (configurable); on Android it locks immediately when returning from background after the timeout
- **Clipboard auto-clear**: copied passwords, usernames and one-time codes are wiped after the configured number of seconds (5–120, default 30), with a visible countdown. **One-time codes are the exception** — they live in the clipboard only as long as the code itself does, so you can never paste one that has already rolled over. The countdown notice only shows on **the entry you copied from** — it does not follow you to other entries
- **Quick unlock**: Windows Hello (PIN / fingerprint / face) or Android fingerprint instead of typing the master password every time; the key stays on this device and is **never** included in backups or sync
- **URL safety**: only `http` / `https` may be opened; `javascript:`, `file:` and friends are blocked

**Everyday use**

- **Two entry kinds**: logins (URL / username / password / TOTP / custom fields / notes) and secure notes (body up to 256 KiB)
- **Two-factor codes (TOTP)**: logins can store a 2FA secret, and the detail view shows the live code; four ways in — camera scan, image file, paste screenshot, screenshot with crop (the last two on desktop)
- **One-click copy**: username, password and one-time code each have their own button — paste and go
- **Password generator**: 8–64 characters (default 16), upper / lower / digits / symbols selectable per class, symbols pickable one by one, ambiguous characters can be excluded; the result can be dropped straight into an entry
- **Folders and tidying**: folders / ungrouped filter, favorites, pinning, drag-to-reorder; right-click on desktop, long-press on mobile to rename or move to a folder
- **Search**: instant filtering inside the vault (title / username / URL, case-insensitive), and "no match" is stated as a match failure rather than an empty vault
- **Password health check**: a health score plus weak / reused / stale passwords, with thresholds (default ≤ 8 characters, 180 days) adjustable in Settings
- **Trash**: soft delete → restore → delete permanently, kept for 30 days, attachments cleaned up alongside entries
- **Attachments**: up to 16 per entry, ≤ 20 MB each, encrypted with the vault; export asks for confirmation first (an exported file is plaintext)
- **Desktop conveniences**: a global hotkey that summons the window with search focused, a standalone quick-search popup (keyboard-driven, copies directly), close-to-tray, optional start-at-login

**Getting data in and out, and across devices**

- **LAN sync**: QR-code pairing + end-to-end encryption; data never touches any cloud server. **Manual trigger only** (why: see "How sync works" below)
- **Conflict arbitration**: when both sides changed the same entry, choose per entry — keep local / take remote / keep both. Nothing is silently overwritten.
- **Migrating from Bitwarden**: import unencrypted JSON / CSV exports (10 columns auto-mapped), same-name folders merged, duplicates skipped, CSV template downloadable; GBK / UTF-16 encodings converted automatically
- **Backup and restore**: export an encrypted whole-vault copy (`.vault`), restore it from the lock screen with "Restore from backup"; Settings always shows when the last successful export happened and how big it was
- **Update check**: off by default; when enabled it asks GitHub for the latest version at most once a day and sends no user data
- **Compact the vault**: reclaim the space used by deleted attachments — and tell you how much is reclaimable before you do it

**Platforms and footprint**

- **Two platforms**: Windows 10/11 (installer / portable) + Android 7.0+ (arm64)
- **Four languages, three themes**: 简体中文 / 繁體中文 / English / ไทย (follows the system language), light / dark / system
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
3. Add login / note entries; give a login a TOTP secret if the site needs two-factor, drag in attachments as needed
4. Optional: set a **global hotkey** in Settings — after that, one keypress in any app lets you search entries and copy a password
5. Multi-device sync: `Devices → Pair new device`, then scan the QR code from the other device (both devices must be on the same LAN). Sync is **manual** — either side can trigger it; see "How sync works"

## 🔑 Two-factor codes (TOTP)

A login entry can carry a TOTP secret. The detail view keeps showing the current 6-digit code and the seconds left in the period, and rolls over to the next code automatically; the copy button at the end of that row hands the code straight to your clipboard. **The clipboard lives exactly as long as the code does** — 12 seconds left means 12 seconds on the clipboard, and under 5 seconds it tells you the code is about to roll over, so you never paste a stale one and blame your password. **That row never hides** — copying a username or password won't tuck it away, so you can always watch it roll over.

### Four ways to get the secret in

Pick by where the code is:

| Where the code is | What to use |
|---|---|
| On another device's screen (phone, second laptop) | **Scan**: open the camera panel and frame the code |
| Already saved as an image file | **From image**: pick the file and it is decoded |
| Just screenshotted, still on the clipboard | **Paste screenshot**: press `Ctrl+V` right in the editor (**no need to click anything first**), or click the button |
| On this very screen, hidden behind PasswordMS itself | **Screenshot**: click it → the app hides itself → the whole screen is captured → the app comes back holding that **frozen still image** → drag a box around the code → release |

A few notes:

- "Paste screenshot" and "Screenshot" are **desktop-only** — Android cannot read bitmaps off the clipboard, so a button there would be a lie. Phones therefore show only "Scan" and "From image".
- The screenshot captures **all monitors at once**, so a code on a secondary display can be framed as well.
- Cropping happens on that **captured still image**: once taken it never changes, so whether the window is visible afterwards makes no difference to what you framed — which is why the app can sit right in front while you drag.
- The crop overlay accepts a reverse drag (bottom-right to top-left); a bare click or a hairline drag is not treated as an error; `Esc` cancels. **A failed decode keeps the overlay open** so you can simply reframe instead of taking another screenshot.
- You can also paste the site's `otpauth://` link straight into the TOTP field, or type the Base32 secret. The secret is test-computed before saving, so an invalid key or an unsupported parameter set is rejected on the spot with a specific reason instead of being stored as a string that can never produce a valid code.

### Supported parameters (one hard limit)

Only **6 digits, 30-second period, SHA1** — what the vast majority of sites use. For a QR code or link asking for 8 digits, a 60-second period or SHA256, the app **refuses explicitly and says why** ("this site uses an 8-digit code, only 6 digits are supported") rather than silently storing it and leaving you staring at a code the site keeps rejecting.

Counter-based HOTP (no time period) is rejected the same way.

### Security

- The secret sits in the encrypted vault like any other field and is encrypted with the whole vault (exports and LAN sync include it).
- Codes are computed **locally**; QR decoding also happens on your machine (jsQR) — the image is never sent anywhere, and nothing goes online.

## 🧩 Where things live

| What you want to do | Where |
|---|---|
| Generate a strong password | The dice button in the editor's password field, or set generator defaults (length / character classes / symbol set) in Settings |
| See which passwords need changing | "Password health" in the sidebar → run the check (weak / reused / stale + suggestions) |
| Move in from another manager | Settings → Import → pick a Bitwarden JSON / CSV (a CSV template is downloadable) |
| Grab a password in any app | Press your global hotkey → quick-search popup → arrow keys to pick → `Enter` copies the password, `Shift+Enter` copies the username |
| Sync phone and desktop | Sync → generate the pairing QR code (scan it on the other device) → tap "Sync now" in the Status tab |
| Both sides edited the same entry | Sync → Conflicts → choose per entry: keep local / take remote / keep both |
| Back up / switch machines | Settings → Export vault (you get a `.vault` encrypted copy); on the new device use "Restore from backup" on the lock screen |
| The vault got big | Settings → Compact vault (reclaims space used by deleted attachments) |
| Change the master password | Settings → Change master password (quick unlock turns itself off and must be re-enabled) |
| The window vanished on close | It minimized to the tray; right-click the tray icon to show / lock / quit |

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
- On successful pairing both screens show the **same session verification code** — compare them literally; only a match means this pairing was not split by a man in the middle
- **On Android the app needs the "Nearby devices" permission**: since Android 17 (API 37), LAN access requires the `ACCESS_LOCAL_NETWORK` runtime grant. Without it the system **silently drops** this app's connections (you see "network timeout" while the same phone's browser and keyboard still reach the LAN just fine). From v1.5.0 the app asks for it on launch — **tap Allow**. If you declined, re-enable it under Settings → Apps → PasswordMS → Permissions → Nearby devices
- **Last-resort fallback**: enable a **mobile hotspot** on the desktop and connect the phone to it before pairing/syncing — inside a hotspot the desktop *is* the phone's gateway, and that path is not affected by the permission above (measured 2026-10-30 on two routers' Wi‑Fi vs hotspot; root-caused 2026-10-31 to the missing permission, fixed in v1.5.0)
- The sync page shows this device's IP, the current listening port, and the recorded address of every paired device — compare both ends to tell "stale address" apart from "peer is away"

## 🔐 Security Model

- The vault is **fully encrypted**; keys are derived from the master password via Argon2id and never written to disk
- LAN sync is end-to-end encrypted: paired devices share the vault key; traffic is unreadable to other devices on the network
- Auto-lock after inactivity; sensitive data in memory is zeroized when locked
- Secrets in the clipboard are wiped after the configured seconds (anything can read the clipboard; this is the only exposure window we can shrink); one-time codes go further and leave as soon as they expire
- This software **never uploads any data** — everything stays on your machine (and the LAN devices you explicitly pair). The single exception: if you turn on "Check for updates" yourself, it asks GitHub for the version number

## ⚠️ Disclaimer / Terms of Use

1. **No master password recovery**: this software has no backdoor and offers no escrow. Forgetting the master password means the data is permanently unrecoverable. Keep it safe and back up regularly via `Settings → Export`.
2. **Uninstalling takes the vault with it**: the vault is never backed up for you — on Android, `allowBackup` is deliberately **disabled** so the vault never reaches a third-party cloud. Deleting the app, switching devices, or reinstalling the OS therefore deletes the local vault file. **Export an encrypted copy first** via `Settings → Export vault` (restore it from the lock screen with "Restore from backup"). When uninstalling from system Settings, Android asks whether to keep app data — tick that box and reinstalling an APK with the same signature restores it in place; uninstalling from an app store or `adb uninstall` shows no such prompt.
3. **Check the exported file size yourself**: Settings keeps showing "Last successful export: <time> (<size>)", and the success toast states the size. **A 0 B backup is no backup at all** — that was exactly the Android export failure before 1.5.2, now fixed. The export is an encrypted copy: whoever holds it still needs the master password, but keep it somewhere safe.
4. **Provided "as is"**: despite thorough testing, the software is not guaranteed defect-free. Use at your own risk and **back up important data regularly**.
5. **Not professional security advice**: this software does not replace a full information-security strategy; use it on trusted devices and keep your OS and WebView2 runtime up to date.
6. **LAN risks**: paired devices share encrypted data. Do not pair over untrusted networks (e.g. public Wi-Fi).
7. **Quick unlock has a different risk model per platform**: on Windows the ceiling is the strength of your local Windows account lock (anyone with that account's credentials can bypass Hello); on Android, adding or removing a fingerprint permanently invalidates the stored key and it must be re-enabled.
8. By using this software you acknowledge and agree to the terms above. The software is released under the [MIT License](LICENSE), which applies alongside these terms.

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

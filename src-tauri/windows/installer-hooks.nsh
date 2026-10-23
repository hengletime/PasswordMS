; PasswordMS NSIS installer hooks (D-82, 2026-10-23)
;
; Root cause: tauri-bundler's nsis/mod.rs generate_resource_data() adds
; WebView2Loader.dll ONLY when the target triple ends with "-gnu".
; Our target is x86_64-pc-windows-gnullvm -> ends_with("-gnu") == false,
; so the NSIS package ships without the DLL and the app cannot start
; ("找不到 WebView2Loader.dll"). The WiX/MSI path globs ALL *.dll from the
; out dir unconditionally, which is why MSI installs always worked.
;
; This hook closes the gap for the NSIS channel only (MSI/APK/portable are
; unaffected). The generated installer.nsi lives at
;   {out_dir}/nsis/{arch}/installer.nsi
; so the DLL sits two levels up at {out_dir}/WebView2Loader.dll.
; If Tauri ever changes that layout, makensis fails loudly here (file not
; found) instead of shipping a broken installer -- that is intentional.
;
; NOTE: comments in this file are ASCII-only on purpose (toolchain writes
; non-ASCII comments get corrupted; see project memory .ps1 lesson).

!macro NSIS_HOOK_POSTINSTALL
  ; App-local WebView2 loader DLL (dynamically loaded, required at startup).
  File "/oname=WebView2Loader.dll" "..\..\WebView2Loader.dll"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove what POSTINSTALL added, so RMDir "$INSTDIR" can succeed.
  Delete "$INSTDIR\WebView2Loader.dll"
!macroend

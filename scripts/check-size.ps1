# PasswordMS size gate (Windows PowerShell / CI).
# Limits: Windows installer <= 10 MB; Android APK <= 12 MB.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\check-size.ps1 [-Target desktop|android|all] [-AllowMissing]
#
# NOTE: This file is intentionally ASCII-only. Chinese comments in .ps1 have been
# observed to get corrupted on write, which breaks PowerShell parsing. All Chinese
# documentation lives in docs/ (see docs/03b-verification.md, docs/04-qa.md).
#
# Gate semantics (QA D-5 revised):
#   - If a platform produced NO artifact at all, the gate FAILS. Previously this
#     script printed "size gate passed" and exited 0 when nothing had been built,
#     which made the size red-line unenforceable in CI.
#     Pass -AllowMissing only when you deliberately know nothing is built yet.
#   - If artifacts exist, each is checked against the limit. Shipping only one
#     bundle format (e.g. NSIS but not MSI) is NOT a failure.
#   - Both the workspace-root target/ and src-tauri/target/ are searched, because
#     in a Cargo workspace the real output directory is the workspace root one.

param(
  [ValidateSet('desktop', 'android', 'all')]
  [string]$Target = 'all',
  [switch]$AllowMissing
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$WinLimitMb = 10
$ApkLimitMb = 12
$rc = 0
$script:rc = 0

function Check-Size {
  param(
    [string]$Label,
    [string[]]$Bases,
    [string[]]$Filters,
    [int]$LimitMb,
    [switch]$BundleOnly
  )

  # Gather ALL artifacts for this platform first, then decide. Reporting failure
  # just because one bundle format is absent (e.g. NSIS built but MSI not) would
  # be a false alarm.
  $items = @()
  foreach ($b in $Bases) {
    if (Test-Path $b) {
      foreach ($flt in $Filters) {
        $items += @(Get-ChildItem -Path $b -Recurse -File -Filter $flt -ErrorAction SilentlyContinue)
      }
    }
  }
  if ($BundleOnly) {
    $items = @($items | Where-Object { $_.FullName -like '*bundle*' })
  }
  $items = @($items | Sort-Object -Property FullName -Unique)

  if ($items.Count -eq 0) {
    if ($AllowMissing) {
      Write-Host "[size] No $Label artifact found (not built). Skipped due to -AllowMissing." -ForegroundColor Cyan
      return
    }
    Write-Host "[FAIL] No $Label artifact found (not built). The size gate FAILS - build first, or pass -AllowMissing deliberately." -ForegroundColor Red
    $script:rc = 1
    return
  }

  foreach ($f in $items) {
    $mb = [math]::Round($f.Length / 1MB, 2)
    if ($mb -le $LimitMb) {
      Write-Host "[ OK ] $Label $($f.FullName) ($mb MB <= $LimitMb MB)" -ForegroundColor Green
    } else {
      Write-Host "[FAIL] $Label $($f.FullName) ($mb MB > $LimitMb MB)" -ForegroundColor Red
      $script:rc = 1
    }
  }
}

if ($Target -eq 'desktop' -or $Target -eq 'all') {
  Write-Host "[size] Checking Windows installer (<= $WinLimitMb MB)" -ForegroundColor Cyan
  Check-Size -Label 'desktop' -Bases @('target', 'src-tauri\target') -Filters @('*.msi', '*-setup.exe') -LimitMb $WinLimitMb -BundleOnly
}
if ($Target -eq 'android' -or $Target -eq 'all') {
  Write-Host "[size] Checking Android APK (<= $ApkLimitMb MB)" -ForegroundColor Cyan
  Check-Size -Label 'android' -Bases @('src-tauri\gen', 'gen') -Filters @('*.apk') -LimitMb $ApkLimitMb
}

$rc = $script:rc
if ($rc -eq 0) {
  Write-Host "[ OK ] Size gate passed." -ForegroundColor Green
} else {
  Write-Host "[FAIL] Size gate NOT passed." -ForegroundColor Red
}
exit $rc

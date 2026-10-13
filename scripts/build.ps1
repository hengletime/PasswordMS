# PasswordMS 一键构建（Windows PowerShell）
# 用法：powershell -ExecutionPolicy Bypass -File scripts\build.ps1 [-Target desktop|android|all]
param(
  [ValidateSet('desktop', 'android', 'all')]
  [string]$Target = 'desktop'
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

function Write-Log  { param($m) Write-Host "[build] $m" -ForegroundColor Cyan }
function Write-Warn { param($m) Write-Host "[warn ] $m" -ForegroundColor Yellow }
function Write-Die  { param($m) Write-Host "[fatal] $m" -ForegroundColor Red; exit 1 }

function Test-Command {
  param($Name)
  return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Build-Frontend {
  Write-Log '构建前端（Vite -> dist/）'
  if (-not (Test-Path 'node_modules')) { npm install }
  npm run build
}

function Build-Desktop {
  Write-Log '构建桌面端（Tauri NSIS/MSI）'
  npm run tauri build
}

if (-not (Test-Command 'node')) { Write-Die '缺少 Node.js' }
if (-not (Test-Command 'npm'))  { Write-Die '缺少 npm' }

switch ($Target) {
  'desktop' {
    if (-not (Test-Command 'cargo')) {
      Write-Warn '未检测到 Rust 工具链（cargo）。桌面构建不可用。'
      Write-Warn '请安装 rustup 后重试：https://rustup.rs/'
      Build-Frontend
      Write-Warn '仅完成前端构建；Rust/桌面产物未生成。'
      exit 0
    }
    Build-Frontend
    Build-Desktop
    & "$Root\scripts\check-size.ps1" -Target desktop
  }
  'android' {
    if (-not (Test-Command 'cargo')) {
      Write-Warn '未检测到 Rust 工具链；Android 构建不可用。'
      Write-Warn '请在已安装 Rust + Android NDK 的环境执行 Android 构建。'
      exit 0
    }
    Build-Frontend
    npm run tauri android build
  }
  'all' {
    if (-not (Test-Command 'cargo')) {
      Write-Warn '未检测到 Rust 工具链；跳过 Rust/桌面/Android 构建。'
      Build-Frontend
      Write-Warn '仅完成前端构建。'
      exit 0
    }
    Build-Frontend
    Build-Desktop
    if (Test-Command 'sdkmanager') {
      npm run tauri android build
    } else {
      Write-Warn '跳过 Android：缺少 Android SDK/NDK。'
    }
    & "$Root\scripts\check-size.ps1" -Target all
  }
}

Write-Log '完成。'

<#
.SYNOPSIS
    StreamApp 개발 실행 스크립트.

.DESCRIPTION
    1. cargo, wasm32-unknown-unknown 타깃, dx 0.7.10, ffmpeg가 있는지 확인한다.
       하나라도 없으면 무엇을 설치해야 하는지 알려 주고 멈춘다.
    2. 레포 루트에서 `dx serve --package web --port <Port>`를 실행한다.
       DB 파일은 레포 루트의 data\streamapp.db에 생긴다(지우고 다시 실행하면 초기 상태로 돌아간다).

.PARAMETER Port
    앱 포트. 기본 8080.

.PARAMETER NoBrowser
    브라우저를 자동으로 열지 않는다.

.EXAMPLE
    .\scripts\dev.ps1
#>
param(
    [int]$Port = 8080,
    [switch]$NoBrowser
)

$ErrorActionPreference = 'Stop'

$RequiredDxVersion = '0.7.10'
$WasmTarget = 'wasm32-unknown-unknown'
$RepoRoot = Split-Path -Parent $PSScriptRoot

# rustup을 막 설치해 PATH가 아직 갱신되지 않은 터미널에서도 동작하게 한다.
$CargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $CargoBin) -and -not (($env:Path -split ';') -contains $CargoBin)) {
    $env:Path = "$CargoBin;$env:Path"
}

function Test-Command([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

$Problems = @()

if (-not (Test-Command 'cargo')) {
    $Problems += 'Rust(cargo)가 없습니다. https://rustup.rs 에서 rustup-init.exe를 실행해 설치하세요. (Visual Studio 2022 Build Tools의 "C++를 사용한 데스크톱 개발"이 필요합니다)'
}

if (-not (Test-Command 'rustup')) {
    $Problems += "rustup이 없어 $WasmTarget 타깃을 확인할 수 없습니다. https://rustup.rs 에서 rustup을 설치한 뒤 'rustup target add $WasmTarget'를 실행하세요."
}
elseif (-not ((& rustup target list --installed) -contains $WasmTarget)) {
    $Problems += "$WasmTarget 타깃이 없습니다. 'rustup target add $WasmTarget'를 실행하세요."
}

$DxInstallHint = "https://github.com/DioxusLabs/dioxus/releases/download/v$RequiredDxVersion/dx-x86_64-pc-windows-msvc.zip 을 받아 dx.exe를 $CargoBin 에 두거나, 'cargo binstall dioxus-cli@$RequiredDxVersion --force'를 실행하세요."
if (-not (Test-Command 'dx')) {
    $Problems += "dx(Dioxus CLI) $RequiredDxVersion 이 없습니다. $DxInstallHint"
}
else {
    $DxVersion = (& dx --version) -join ' '
    if ($DxVersion -notmatch "\b$([regex]::Escape($RequiredDxVersion))\b") {
        $Problems += "dx $RequiredDxVersion 이 필요한데 지금은 '$DxVersion' 입니다. $DxInstallHint"
    }
}

if (-not (Test-Command 'ffmpeg')) {
    $Problems += "ffmpeg가 없습니다(테스트 송출에 씁니다). 'winget install --id Gyan.FFmpeg -e'로 설치하거나 https://ffmpeg.org/download.html 에서 받아 PATH에 넣으세요."
}

if ($Problems.Count -gt 0) {
    Write-Host '개발 환경에 필요한 도구가 없어 멈춥니다. 아래를 설치한 뒤 새 터미널에서 다시 실행하세요.' -ForegroundColor Red
    foreach ($Problem in $Problems) {
        Write-Host " - $Problem" -ForegroundColor Yellow
    }
    exit 1
}

Write-Host "도구 확인 완료. http://127.0.0.1:$Port 에서 앱을 띄웁니다." -ForegroundColor Green

$DxArgs = @('serve', '--package', 'web', '--port', $Port)
if ($NoBrowser) {
    $DxArgs += @('--open', 'false')
}

Push-Location $RepoRoot
try {
    & dx @DxArgs
    $DxExitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}
exit $DxExitCode

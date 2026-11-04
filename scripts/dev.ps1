<#
.SYNOPSIS
    StreamApp 개발 실행 스크립트.

.DESCRIPTION
    1. cargo, wasm32-unknown-unknown 타깃, dx 0.7.10, ffmpeg가 있는지 확인한다.
       하나라도 없으면 무엇을 설치해야 하는지 알려 주고 멈춘다.
    2. MediaMTX v1.21.3이 없으면 공식 GitHub 릴리스에서 git에서 제외한 .tools\ 폴더로 내려받는다
       (체크섬을 확인한다).
    3. 레포의 설정(tools\mediamtx\mediamtx.yml)으로 MediaMTX를 실행한다.
       RTMP 1935, HLS 8888, WebRTC 8889(휴대폰 방송, ICE 8189), Control API 9997을 쓰고, 로그는 .tools\mediamtx.log에 쓴다.
    4. 레포 루트에서 `dx serve --package web --port <Port>`를 실행한다.
       DB 파일은 레포 루트의 data\streamapp.db에 생긴다(지우고 다시 실행하면 초기 상태로 돌아간다).
       앱은 시작할 때 MediaMTX Control API(STREAMAPP_MEDIAMTX_API, 기본 http://127.0.0.1:9997)로
       지금 송출 중인 채널을 조회해 라이브를 맞춘다.
       dx serve가 끝나면(Ctrl+C 포함) MediaMTX도 끈다.

.PARAMETER Port
    앱 포트. 기본 8080. MediaMTX가 부르는 앱 주소(인증, 송출 시작·종료 훅)와 HLS가 허용하는 출처도
    이 포트로 맞춘다.

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

Write-Host '도구 확인 완료.' -ForegroundColor Green

# ---- MediaMTX: 실행 파일은 git에서 제외한 .tools\ 에, 설정은 레포의 tools\mediamtx\ 에 있다(ADR 0002). ----
$MtxVersion = 'v1.21.3'
# 릴리스의 checksums.sha256에 적힌 mediamtx_v1.21.3_windows_amd64.zip의 SHA-256. 버전을 바꾸면 함께 바꾼다.
$MtxZipSha256 = 'b2f5246b07cb72961df03b8d5e8cad43dc657bcf3f49a87bb031bdf22721af74'
$MtxDir = Join-Path $RepoRoot ".tools\mediamtx-$MtxVersion"
$MtxExe = Join-Path $MtxDir 'mediamtx.exe'
$MtxConf = Join-Path $RepoRoot 'tools\mediamtx\mediamtx.yml'

if (-not (Test-Path $MtxExe)) {
    $ZipName = "mediamtx_${MtxVersion}_windows_amd64.zip"
    $ZipUrl = "https://github.com/bluenviron/mediamtx/releases/download/$MtxVersion/$ZipName"
    $Zip = Join-Path $RepoRoot ".tools\$ZipName"
    $Staging = "$MtxDir.partial"
    Write-Host "MediaMTX $MtxVersion 이 없어 내려받습니다: $ZipUrl"
    try {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Zip) | Out-Null
        # PowerShell 5.1의 진행 표시줄은 내려받기를 크게 느리게 한다.
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -UseBasicParsing -Uri $ZipUrl -OutFile $Zip
        $ZipHash = (Get-FileHash -Algorithm SHA256 -Path $Zip).Hash
        if ($ZipHash -ne $MtxZipSha256) {
            throw "체크섬이 다릅니다(기대 $MtxZipSha256, 실제 $ZipHash)."
        }
        # 압축을 다 푼 뒤에 폴더 이름을 바꿔, 중간에 멈춰도 반쯤 풀린 폴더를 쓰지 않게 한다.
        if (Test-Path $Staging) {
            Remove-Item -Recurse -Force $Staging
        }
        Expand-Archive -Path $Zip -DestinationPath $Staging
        Move-Item -Path $Staging -Destination $MtxDir
    }
    catch {
        Write-Host "MediaMTX를 내려받지 못해 멈춥니다: $_" -ForegroundColor Red
        Write-Host "직접 받으려면 $ZipUrl 의 압축을 $MtxDir 에 푸세요." -ForegroundColor Yellow
        exit 1
    }
    finally {
        Remove-Item -Force -ErrorAction SilentlyContinue $Zip
    }
}

# 여기부터는 네이티브 명령(mediamtx, dx)의 성공 여부를 종료 코드로 판단한다. 'Stop'으로 두면 출력을
# 파일로 돌렸을 때 PowerShell 5.1이 stderr 한 줄(예: dx의 "warning: Waiting for cargo-metadata...")을
# 오류로 보고 스크립트를 멈춘다.
$ErrorActionPreference = 'Continue'

& $MtxExe "--validate-conf=$MtxConf"
if ($LASTEXITCODE -ne 0) {
    Write-Host "MediaMTX 설정이 올바르지 않아 멈춥니다: $MtxConf" -ForegroundColor Red
    exit 1
}

# MediaMTX 로그는 화면 대신 파일에 쓴다(dx serve 화면과 섞이지 않게). 실행할 때마다 새로 쓴다.
$MtxLog = Join-Path $RepoRoot '.tools\mediamtx.log'
Remove-Item -Force -ErrorAction SilentlyContinue $MtxLog

# 송출 시작·종료 훅 명령. MediaMTX가 셸 없이 실행하므로 curl.exe를 이름으로 부른다.
# $MTX_PATH(채널 ID)는 MediaMTX가 실행할 때 바꿔 넣으므로 작은따옴표로 글자 그대로 넘긴다.
# 아래 훅 경로(.../hooks/available, .../hooks/unavailable)는 Rust 상수 api::mediamtx::AVAILABLE_HOOK_PATH,
# api::mediamtx::UNAVAILABLE_HOOK_PATH와 같아야 한다.
$HookCommand = "curl.exe -s -X POST http://127.0.0.1:$Port/internal/mediamtx/hooks"

# 설정 파일의 값을 환경 변수(MTX_<대문자 키>)로 덮어쓴다: 앱 포트에 맞춘 주소와 로그 파일.
# MediaMTX 프로세스에만 넘기고 바로 되돌린다. 인증 경로(/internal/mediamtx/auth)는 Rust 상수
# api::mediamtx::AUTH_PATH와 같아야 한다.
$MtxEnv = @{
    MTX_AUTHHTTPADDRESS               = "http://127.0.0.1:$Port/internal/mediamtx/auth"
    MTX_HLSALLOWORIGINS               = "http://localhost:$Port,http://127.0.0.1:$Port"
    MTX_PATHDEFAULTS_RUNONAVAILABLE   = "$HookCommand/available?path=" + '$MTX_PATH'
    MTX_PATHDEFAULTS_RUNONUNAVAILABLE = "$HookCommand/unavailable?path=" + '$MTX_PATH'
    MTX_LOGDESTINATIONS               = 'file'
    MTX_LOGFILE                       = $MtxLog
}
$SavedEnv = @{}
foreach ($Name in $MtxEnv.Keys) {
    $SavedEnv[$Name] = [Environment]::GetEnvironmentVariable($Name, 'Process')
    [Environment]::SetEnvironmentVariable($Name, $MtxEnv[$Name], 'Process')
}
try {
    # 같은 콘솔에 붙여 띄운다: 콘솔 창을 닫거나 Ctrl+C를 누르면 MediaMTX도 함께 끝나 혼자 남지 않는다.
    $Mtx = Start-Process -FilePath $MtxExe -ArgumentList "`"$MtxConf`"" -WorkingDirectory $RepoRoot -NoNewWindow -PassThru
}
finally {
    foreach ($Name in $SavedEnv.Keys) {
        [Environment]::SetEnvironmentVariable($Name, $SavedEnv[$Name], 'Process')
    }
}

Start-Sleep -Seconds 1
if ($Mtx.HasExited) {
    Write-Host "MediaMTX가 바로 종료되어 멈춥니다. 포트 1935, 8888, 8889, 8189, 9997을 다른 프로그램이 쓰고 있지 않은지 확인하세요. 로그($MtxLog):" -ForegroundColor Red
    Get-Content -ErrorAction SilentlyContinue -Tail 20 $MtxLog
    exit 1
}
# 포트는 mediamtx.yml의 값이다. HLS 8888은 Rust 상수 api::mediamtx::HLS_BASE_URL(시청 페이지의 재생 주소)과,
# Control API 9997은 api::server::mediamtx_api::DEFAULT_ADDRESS와 같아야 한다.
Write-Host "MediaMTX $MtxVersion 실행 중(RTMP 1935, HLS 8888, WebRTC 8889·ICE 8189, Control API 9997). 로그: $MtxLog" -ForegroundColor Green
Write-Host "  로그 보기: Get-Content -Wait $MtxLog"
Write-Host "  테스트 송출: .\scripts\test-publish.ps1 -ChannelId <채널 ID> -StreamKey <송출 키>"
Write-Host "http://127.0.0.1:$Port 에서 앱을 띄웁니다." -ForegroundColor Green

$DxArgs = @('serve', '--package', 'web', '--port', $Port)
if ($NoBrowser) {
    $DxArgs += @('--open', 'false')
}

# 앱은 시작할 때(다시 빌드되어 재시작할 때 포함) 이 주소의 MediaMTX Control API로 지금 송출 중인 채널을
# 조회해 라이브를 맞춘다. 위에서 띄운 MediaMTX의 apiAddress(:9997)와 같아야 한다(앱의 기본값은 Rust 상수
# api::server::mediamtx_api::DEFAULT_ADDRESS). dx serve와 앱에만 넘기고 되돌린다.
$SavedMtxApiAddress = $env:STREAMAPP_MEDIAMTX_API
$env:STREAMAPP_MEDIAMTX_API = 'http://127.0.0.1:9997'

$DxExitCode = 1
Push-Location $RepoRoot
try {
    & dx @DxArgs
    $DxExitCode = $LASTEXITCODE
}
finally {
    Pop-Location
    $env:STREAMAPP_MEDIAMTX_API = $SavedMtxApiAddress
    if (-not $Mtx.HasExited) {
        Stop-Process -Id $Mtx.Id -Force
    }
}
exit $DxExitCode

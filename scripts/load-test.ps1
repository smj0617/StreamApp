<#
.SYNOPSIS
    로컬 오토스케일(과제 ⑤): k6로 부하를 주고, 그동안과 그 뒤의 HPA·파드 수 변화를 파일로 남긴다.

.DESCRIPTION
    1. scripts\k8s-watch.ps1을 백그라운드 작업으로 띄운다(부하 시간 + CoolDownSeconds 동안 기록).
    2. Docker로 k6(grafana/k6:1.8.3)를 돌려 loadtest\autoscale.js를 실행한다.
       k6 컨테이너를 kind의 Docker 네트워크('kind')에 붙여 노드의 앱 NodePort(http://streamapp-control-plane:30080)로
       바로 보낸다. 요청마다 새 연결을 맺으므로, 호스트 포트(host.docker.internal:18080)를 거치면 Docker Desktop의
       포트 중계가 연결을 감당하지 못해 dial timeout이 난다.
       단계: RampSeconds 동안 Vus까지 늘리기 → HoldSeconds 유지 → 10초 동안 줄이기.
    3. 부하가 끝난 뒤에도 CoolDownSeconds 동안 기록해 파드가 다시 줄어드는지(스케일 인) 본다.
    기록 파일은 loadtest\out\autoscale-<시각>.log(git 제외), k6 요약은 loadtest\out\k6-<시각>.txt.

.PARAMETER Vus
    동시 가상 사용자 수. 기본 40.

.EXAMPLE
    .\scripts\load-test.ps1
    .\scripts\load-test.ps1 -Vus 60 -HoldSeconds 300
#>
param(
    [int]$Vus = 40,
    [int]$RampSeconds = 30,
    [int]$HoldSeconds = 240,
    [int]$CoolDownSeconds = 240,
    [string]$BaseUrl = 'http://streamapp-control-plane:30080',
    [string]$DockerNetwork = 'kind'
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$K6Image = 'grafana/k6:1.8.3'

$DockerBin = Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\resources\bin'
if ((Test-Path $DockerBin) -and -not (($env:Path -split ';') -contains $DockerBin)) {
    $env:Path = "$env:Path;$DockerBin"
}

$OutDir = Join-Path $RepoRoot 'loadtest\out'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$WatchLog = Join-Path $OutDir "autoscale-$Stamp.log"
$K6Log = Join-Path $OutDir "k6-$Stamp.txt"

$WatchSeconds = $RampSeconds + $HoldSeconds + 10 + $CoolDownSeconds
$WatchScript = Join-Path $PSScriptRoot 'k8s-watch.ps1'
Write-Host "관찰 시작(약 $WatchSeconds 초): $WatchLog"
$Watcher = Start-Job -ScriptBlock {
    param($Script, $Seconds, $Log)
    & powershell -NoProfile -ExecutionPolicy Bypass -File $Script -DurationSeconds $Seconds -OutFile $Log | Out-Null
} -ArgumentList $WatchScript, $WatchSeconds, $WatchLog

Start-Sleep -Seconds 5
Write-Host "부하 시작: 가상 사용자 $Vus, 늘리기 $RampSeconds 초, 유지 $HoldSeconds 초 → $BaseUrl"
$ScriptsDir = Join-Path $RepoRoot 'loadtest'
& docker run --rm -i --network $DockerNetwork `
    -v "${ScriptsDir}:/scripts:ro" `
    -e "BASE_URL=$BaseUrl" -e "VUS=$Vus" -e "RAMP=${RampSeconds}s" -e "HOLD=${HoldSeconds}s" `
    $K6Image run --quiet /scripts/autoscale.js |
    ForEach-Object { $_; Add-Content -Path $K6Log -Value $_ -Encoding UTF8 }
$K6Exit = $LASTEXITCODE
Write-Host "부하 끝(k6 종료 코드 $K6Exit). 스케일 인을 보려고 $CoolDownSeconds 초 더 기록합니다..."

Wait-Job $Watcher | Out-Null
Receive-Job $Watcher | Out-Null
Remove-Job $Watcher

Write-Host ''
Write-Host "관찰 기록: $WatchLog" -ForegroundColor Green
Write-Host "k6 요약:   $K6Log" -ForegroundColor Green
Get-Content -Path $WatchLog -Encoding UTF8

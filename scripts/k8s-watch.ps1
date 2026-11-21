<#
.SYNOPSIS
    로컬 오토스케일(과제 ⑤): HPA와 앱 파드 수의 변화를 주기적으로 화면과 파일에 남긴다.

.DESCRIPTION
    IntervalSeconds마다 한 줄씩 적는다:
      - HPA(kubectl get hpa streamapp): 현재/목표 CPU 사용률, 최소·최대, 지금 파드 수
      - 앱 파드: 전체 수와 Ready 수, 파드별 CPU 사용량(kubectl top, metrics-server)
      - 요청 분산: http://localhost:18080/healthz 에 Samples번 새 연결로 요청해 응답 헤더 x-streamapp-instance
        (답한 파드 이름)를 센다.
    DurationSeconds가 지나거나 Ctrl+C로 멈춘다. 파일은 loadtest\out\ 아래(git 제외)에 생긴다.

.PARAMETER IntervalSeconds
    기록 간격(초). 기본 10.

.PARAMETER DurationSeconds
    기록할 시간(초). 0이면 Ctrl+C까지. 기본 0.

.PARAMETER Samples
    한 번에 보낼 분산 확인 요청 수. 0이면 보내지 않는다. 기본 12.

.PARAMETER OutFile
    기록 파일. 기본 loadtest\out\autoscale-<시각>.log

.EXAMPLE
    .\scripts\k8s-watch.ps1 -DurationSeconds 600
#>
param(
    [int]$IntervalSeconds = 10,
    [int]$DurationSeconds = 0,
    [int]$Samples = 12,
    [string]$OutFile = '',
    [string]$AppUrl = 'http://localhost:18080'
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Context = 'kind-streamapp'
$Namespace = 'streamapp'

$DockerBin = Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\resources\bin'
if ((Test-Path $DockerBin) -and -not (($env:Path -split ';') -contains $DockerBin)) {
    $env:Path = "$env:Path;$DockerBin"
}

if (-not $OutFile) {
    $OutDir = Join-Path $RepoRoot 'loadtest\out'
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    $OutFile = Join-Path $OutDir ("autoscale-{0}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
}

function Write-Line([string]$Text) {
    Write-Host $Text
    Add-Content -Path $OutFile -Value $Text -Encoding UTF8
}

# 외부 명령의 출력(줄 배열). 실패해도 멈추지 않고 빈 배열을 돌려준다. 한 줄이면 문자열로 풀리므로 부르는 쪽이 @()로 감싼다.
function Get-Lines([scriptblock]$Command) {
    $ErrorActionPreference = 'Continue'
    $Lines = @(& $Command 2>$null)
    if ($LASTEXITCODE -ne 0) { return @() }
    return $Lines
}

# 요청 Samples번의 응답 헤더 x-streamapp-instance를 세어 "파드=횟수" 목록을 돌려준다(파드 이름 끝 5글자만).
function Get-Spread {
    if ($Samples -le 0) { return '' }
    $Counts = @{}
    $Failed = 0
    for ($i = 0; $i -lt $Samples; $i++) {
        try {
            $Response = Invoke-WebRequest -UseBasicParsing -DisableKeepAlive -TimeoutSec 5 -Uri "$AppUrl/healthz"
            $Name = [string]$Response.Headers['x-streamapp-instance']
            if (-not $Name) { $Name = '?' }
            $Short = if ($Name.Length -gt 5) { $Name.Substring($Name.Length - 5) } else { $Name }
            $Counts[$Short] = 1 + [int]$Counts[$Short]
        }
        catch {
            $Failed++
        }
    }
    $Parts = @($Counts.Keys | Sort-Object | ForEach-Object { "$_=$($Counts[$_])" })
    if ($Failed -gt 0) { $Parts += "실패=$Failed" }
    return ($Parts -join ' ')
}

Write-Line ("# StreamApp 오토스케일 관찰 시작 {0} (간격 {1}초, 분산 확인 요청 {2}번/회)" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $IntervalSeconds, $Samples)
Write-Line '# 시각     | HPA CPU(현재/목표) | HPA 파드 | 앱 파드(Ready/전체) | 파드별 CPU(kubectl top) | 요청 분산(파드 이름 끝 5글자=응답 수)'

$Start = Get-Date
while ($true) {
    $Now = Get-Date -Format 'HH:mm:ss'

    # HPA: NAME REFERENCE TARGETS MINPODS MAXPODS REPLICAS AGE
    $Hpa = @(Get-Lines { kubectl --context $Context -n $Namespace get hpa streamapp --no-headers })
    $Targets = '?'; $Replicas = '?'
    if ($Hpa.Count -gt 0) {
        $Cols = ($Hpa[0] -split '\s+')
        # TARGETS는 "cpu: 12%/50%" 처럼 공백이 들어갈 수 있다. 뒤에서부터 센다.
        $Replicas = $Cols[$Cols.Count - 2]
        $Targets = (($Cols[2..($Cols.Count - 5)]) -join ' ') -replace '^cpu:\s*', ''
    }

    # 앱 파드: NAME READY STATUS RESTARTS AGE
    $Pods = @(Get-Lines { kubectl --context $Context -n $Namespace get pods -l app=streamapp --no-headers })
    $Total = 0; $Ready = 0
    foreach ($Pod in $Pods) {
        $Cols = ($Pod -split '\s+')
        if ($Cols[2] -eq 'Terminating') { continue }
        $Total++
        if ($Cols[1] -eq '1/1') { $Ready++ }
    }

    # 파드별 CPU: NAME CPU MEMORY
    $Top = @(Get-Lines { kubectl --context $Context -n $Namespace top pods -l app=streamapp --no-headers })
    $Cpu = @($Top | ForEach-Object {
            $Cols = ($_ -split '\s+')
            $Name = $Cols[0]
            $Short = if ($Name.Length -gt 5) { $Name.Substring($Name.Length - 5) } else { $Name }
            "$Short=$($Cols[1])"
        }) -join ' '

    $Spread = Get-Spread
    Write-Line ("{0} | {1,-17} | {2,-8} | {3,-19} | {4} | {5}" -f $Now, $Targets, $Replicas, "$Ready/$Total", $Cpu, $Spread)

    if ($DurationSeconds -gt 0 -and ((Get-Date) - $Start).TotalSeconds -ge $DurationSeconds) { break }
    Start-Sleep -Seconds $IntervalSeconds
}
Write-Line ("# 관찰 끝 {0}. 기록 파일: {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $OutFile)

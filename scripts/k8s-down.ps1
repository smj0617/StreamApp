<#
.SYNOPSIS
    로컬 오토스케일(과제 ⑤): kind 클러스터 'streamapp'을 지운다.

.DESCRIPTION
    클러스터(노드 컨테이너)와 그 안의 모든 것(공유 볼륨의 DB·녹화방송 영상 포함)이 사라진다.
    .tools\kind.exe와 앱 이미지(streamapp:local)는 남긴다. 다시 띄우려면 scripts\k8s-up.ps1.

.EXAMPLE
    .\scripts\k8s-down.ps1
#>
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ClusterName = 'streamapp'
$Kind = Join-Path $RepoRoot '.tools\kind.exe'

$DockerBin = Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\resources\bin'
if ((Test-Path $DockerBin) -and -not (($env:Path -split ';') -contains $DockerBin)) {
    $env:Path = "$env:Path;$DockerBin"
}

if (-not (Test-Path $Kind)) {
    throw "$Kind 이 없습니다. 클러스터를 만든 적이 없으면 지울 것도 없습니다."
}
$Clusters = @(& $Kind get clusters)
if (-not ($Clusters -contains $ClusterName)) {
    Write-Host "kind 클러스터 '$ClusterName'이 없습니다."
    exit 0
}
& $Kind delete cluster --name $ClusterName
if ($LASTEXITCODE -ne 0) {
    throw "클러스터 지우기 실패(종료 코드 $LASTEXITCODE)"
}
Write-Host "kind 클러스터 '$ClusterName'을 지웠습니다." -ForegroundColor Green

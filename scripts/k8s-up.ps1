<#
.SYNOPSIS
    로컬 오토스케일(과제 ⑤): kind 클러스터를 띄우고 StreamApp을 여러 인스턴스로 배포한다.

.DESCRIPTION
    1. docker(Docker Desktop)와 kubectl이 있는지, Docker 데몬이 켜져 있는지 확인한다.
    2. kind v0.33.0이 없으면 공식 GitHub 릴리스에서 git에서 제외한 .tools\ 폴더로 내려받는다(체크섬 확인).
    3. kind 클러스터 'streamapp'(k8s\kind-cluster.yaml, 노드 1개)가 없으면 만든다.
    4. metrics-server v0.9.1(k8s\metrics-server, --kubelet-insecure-tls 덧붙임)을 적용한다.
    5. 앱 이미지를 빌드하고(docker build -t streamapp:local .) kind 노드에 넣는다(kind load docker-image).
    6. k8s\app\secret.env가 없으면 secret.env.example을 복사한다(로컬 시험 값).
    7. k8s\app(네임스페이스 streamapp: 앱 Deployment·Service·HPA, MediaMTX, Redis, 공유 볼륨)을 적용하고
       모두 준비될 때까지 기다린다.

    끝나면 http://localhost:18080 으로 접속한다. 클러스터를 지우려면 scripts\k8s-down.ps1.

.PARAMETER SkipBuild
    이미지를 다시 빌드하지 않고 이미 있는 streamapp:local을 노드에 넣는다.

.EXAMPLE
    .\scripts\k8s-up.ps1
#>
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ClusterName = 'streamapp'
$Context = "kind-$ClusterName"
$Namespace = 'streamapp'
$Image = 'streamapp:local'

# kind 버전과 kind-windows-amd64의 SHA-256(릴리스의 kind-windows-amd64.sha256sum). 버전을 바꾸면 함께 바꾼다.
$KindVersion = 'v0.33.0'
$KindSha256 = '4B22ADAA135368C5A465D56BBD8E520CBEA87272A06CA00B6078E7B81515C9FC'

# Docker Desktop이 설치한 docker·kubectl이 PATH에 없을 때를 대비한다.
$DockerBin = Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\resources\bin'
if ((Test-Path $DockerBin) -and -not (($env:Path -split ';') -contains $DockerBin)) {
    $env:Path = "$env:Path;$DockerBin"
}

function Test-Command([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

# 외부 프로그램을 실행하고 실패(0이 아닌 종료 코드)하면 멈춘다.
function Invoke-Checked {
    param([string]$What, [scriptblock]$Command)
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$What 실패(종료 코드 $LASTEXITCODE)"
    }
}

if (-not (Test-Command 'docker')) {
    throw 'docker가 없습니다. Docker Desktop을 설치하세요: https://www.docker.com/products/docker-desktop/'
}
if (-not (Test-Command 'kubectl')) {
    throw 'kubectl이 없습니다. Docker Desktop에 함께 들어 있습니다(resources\bin). 또는 winget install -e --id Kubernetes.kubectl'
}
& docker info --format '{{.ServerVersion}}' | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'Docker 데몬에 연결할 수 없습니다. Docker Desktop을 켜고 엔진이 실행 중(Engine running)인지 확인하세요.'
}

# ---------- kind 내려받기 ----------
$ToolsDir = Join-Path $RepoRoot '.tools'
$Kind = Join-Path $ToolsDir 'kind.exe'
if (Test-Path $Kind) {
    $Hash = (Get-FileHash -Algorithm SHA256 -Path $Kind).Hash
    if ($Hash -ne $KindSha256) {
        Write-Host "기존 $Kind 의 체크섬이 $KindVersion 과 달라 다시 내려받습니다."
        Remove-Item $Kind
    }
}
if (-not (Test-Path $Kind)) {
    New-Item -ItemType Directory -Force -Path $ToolsDir | Out-Null
    $Url = "https://github.com/kubernetes-sigs/kind/releases/download/$KindVersion/kind-windows-amd64"
    Write-Host "kind $KindVersion 내려받는 중: $Url"
    $Download = "$Kind.download"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Download
    $Hash = (Get-FileHash -Algorithm SHA256 -Path $Download).Hash
    if ($Hash -ne $KindSha256) {
        Remove-Item $Download
        throw "kind 체크섬이 다릅니다(기대 $KindSha256, 실제 $Hash)."
    }
    Move-Item $Download $Kind
}
Write-Host "kind: $((& $Kind version) -join ' ')"

# ---------- 클러스터 ----------
$Clusters = @(& $Kind get clusters)
if ($Clusters -contains $ClusterName) {
    Write-Host "kind 클러스터 '$ClusterName'이 이미 있습니다."
}
else {
    Write-Host "kind 클러스터 '$ClusterName' 만드는 중(노드 이미지를 처음 받으면 몇 분 걸립니다)..."
    Invoke-Checked 'kind 클러스터 만들기' {
        & $Kind create cluster --config (Join-Path $RepoRoot 'k8s\kind-cluster.yaml') --wait 120s
    }
}

# ---------- metrics-server ----------
Write-Host 'metrics-server 적용...'
Invoke-Checked 'metrics-server 적용' {
    & kubectl --context $Context apply -k (Join-Path $RepoRoot 'k8s\metrics-server')
}

# ---------- 앱 이미지 ----------
if (-not $SkipBuild) {
    Write-Host "앱 이미지 빌드: $Image (처음에는 Rust 빌드로 10~20분 걸립니다)"
    Invoke-Checked '이미지 빌드' { & docker build -t $Image $RepoRoot }
}
Write-Host "이미지를 kind 노드에 넣는 중: $Image"
Invoke-Checked '이미지 넣기' { & $Kind load docker-image $Image --name $ClusterName }

# ---------- 비밀 값 ----------
$SecretEnv = Join-Path $RepoRoot 'k8s\app\secret.env'
if (-not (Test-Path $SecretEnv)) {
    Copy-Item (Join-Path $RepoRoot 'k8s\app\secret.env.example') $SecretEnv
    Write-Host "k8s\app\secret.env가 없어 예시 값(시험용 비밀번호 test1234)으로 만들었습니다."
}

# ---------- 앱 적용 ----------
# Windows PowerShell 5.1은 Stop일 때 네이티브 명령의 stderr 리디렉션을 오류로 바꾸므로 잠깐 Continue로 둔다.
$ErrorActionPreference = 'Continue'
& kubectl --context $Context -n $Namespace get deployment streamapp 2>$null | Out-Null
$AppExisted = ($LASTEXITCODE -eq 0)
$ErrorActionPreference = 'Stop'
Write-Host 'StreamApp 매니페스트 적용(k8s\app)...'
Invoke-Checked '매니페스트 적용' {
    & kubectl --context $Context apply -k (Join-Path $RepoRoot 'k8s\app')
}
if ($AppExisted) {
    # 같은 태그(streamapp:local)로 새 이미지를 넣었으므로 파드를 새로 띄워 새 이미지를 쓰게 한다.
    Invoke-Checked '앱 다시 띄우기' {
        & kubectl --context $Context -n $Namespace rollout restart deployment/streamapp
    }
}

Write-Host '준비될 때까지 기다리는 중...'
Invoke-Checked 'metrics-server 준비' {
    & kubectl --context $Context -n kube-system rollout status deployment/metrics-server --timeout=180s
}
foreach ($Deployment in @('redis', 'mediamtx', 'streamapp')) {
    Invoke-Checked "$Deployment 준비" {
        & kubectl --context $Context -n $Namespace rollout status "deployment/$Deployment" --timeout=300s
    }
}

& kubectl --context $Context -n $Namespace get pods,svc,hpa,pvc -o wide
Write-Host ''
Write-Host '준비됐습니다.' -ForegroundColor Green
Write-Host '  앱:          http://localhost:18080   (기본 사용자 철수·영희·민수, 비밀번호는 k8s\app\secret.env)'
Write-Host '  OBS 송출:    rtmp://localhost/<채널ID>?key=<송출 키>'
Write-Host '  상태 보기:   .\scripts\k8s-watch.ps1        부하 시험: .\scripts\load-test.ps1'
Write-Host '  지우기:      .\scripts\k8s-down.ps1'

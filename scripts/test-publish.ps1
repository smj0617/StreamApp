<#
.SYNOPSIS
    OBS 없이 채널 하나에 테스트 화면을 송출한다.

.DESCRIPTION
    ffmpeg로 컬러바 위에 현재 시각을 그린 화면(1280x720, 30fps, H.264)과 1kHz 사인파 소리(AAC)를 만들어
    RTMP로 MediaMTX에 송출한다. 주소는 <Server>/<채널 ID>?key=<송출 키> 이다.
    Ctrl+C로 멈춘다. 송출 키가 틀리면 MediaMTX가 거절해 ffmpeg가 오류로 끝난다(종료 코드가 0이 아니다).
    MediaMTX는 scripts\dev.ps1이 띄운다. 재생: http://localhost:8888/<채널 ID>/index.m3u8

.PARAMETER ChannelId
    송출할 채널의 ID.

.PARAMETER StreamKey
    그 채널의 송출 키.

.PARAMETER Server
    RTMP 서버 주소. 기본 rtmp://localhost

.EXAMPLE
    .\scripts\test-publish.ps1 -ChannelId abcdef123456 -StreamKey <송출 키>
#>
param(
    [Parameter(Mandatory = $true)][string]$ChannelId,
    [Parameter(Mandatory = $true)][string]$StreamKey,
    [string]$Server = 'rtmp://localhost'
)

# ffmpeg는 경고와 오류를 stderr에 쓴다. 성공 여부는 종료 코드로 판단한다
# ('Stop'이면 출력을 파일로 돌렸을 때 PowerShell 5.1이 stderr 한 줄에 스크립트를 멈춘다).
$ErrorActionPreference = 'Continue'

if (-not (Get-Command 'ffmpeg' -ErrorAction SilentlyContinue)) {
    Write-Host "ffmpeg가 없습니다. 'winget install --id Gyan.FFmpeg -e'로 설치하거나 https://ffmpeg.org/download.html 에서 받아 PATH에 넣으세요." -ForegroundColor Red
    exit 1
}

$ChannelUrl = "$($Server.TrimEnd('/'))/$ChannelId"
$Url = "${ChannelUrl}?key=${StreamKey}"

# 화면 아래쪽 가운데에 현재 시각(기본 형식 %Y-%m-%d %H:%M:%S)을 그린다.
# 필터 안에서 ':'는 옵션 구분자라 글꼴 경로의 'C:'를 'C\:'로 쓴다. 작은따옴표 문자열 안의 ''는 작은따옴표 하나다.
# 공백을 넣지 않는다: PowerShell 5.1이 네이티브 인자를 큰따옴표로 감싸며 따옴표가 깨질 수 있다.
$DrawClock = 'drawtext=fontfile=''C\:/Windows/Fonts/arial.ttf'':text=''%{localtime}'':fontsize=56:fontcolor=white:box=1:boxcolor=black@0.6:boxborderw=12:x=(w-text_w)/2:y=h-text_h-40'

Write-Host "송출 시작: $ChannelUrl (멈추려면 Ctrl+C)" -ForegroundColor Green

# -re: 입력을 실시간 속도로 만든다(lavfi는 기본적으로 실시간보다 빠르다).
# -g 30: 키프레임을 1초마다 넣어 HLS 지연을 줄인다.
& ffmpeg -hide_banner -loglevel warning `
    -re -f lavfi -i 'smptehdbars=size=1280x720:rate=30' `
    -re -f lavfi -i 'sine=frequency=1000:sample_rate=48000' `
    -vf $DrawClock `
    -c:v libx264 -preset veryfast -tune zerolatency -pix_fmt yuv420p `
    -g 30 -keyint_min 30 -sc_threshold 0 -b:v 2500k -maxrate 2500k -bufsize 5000k `
    -c:a aac -b:a 128k -ar 48000 -ac 2 `
    -f flv $Url
$FfmpegExitCode = $LASTEXITCODE

if ($FfmpegExitCode -ne 0) {
    Write-Host "송출이 오류로 끝났습니다(종료 코드 $FfmpegExitCode). 채널 ID와 송출 키, MediaMTX 실행 여부를 확인하세요." -ForegroundColor Red
}
exit $FfmpegExitCode

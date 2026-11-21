#!/usr/bin/env bash
# StreamApp 서버 최초 준비(Ubuntu 24.04, OCI Always Free VM). 서버에서 root 권한으로 한 번 실행한다.
# 다시 실행해도 된다(이미 한 일은 건너뛰고, 설정 파일은 레포 것으로 다시 깐다. streamapp.env는 덮어쓰지 않는다).
#
# 사용법(로컬에서 deploy/ 폴더를 서버에 올린 뒤):
#   scp -r deploy ubuntu@<공인 IP>:~/streamapp-deploy
#   ssh ubuntu@<공인 IP>
#   sudo bash ~/streamapp-deploy/setup-server.sh --domain stream.example.xyz [--public-ip 1.2.3.4] [--no-rtmp]
#
# 하는 일:
#   1. 패키지 설치(curl, iptables-persistent 등)와 Caddy 공식 apt 저장소에서 Caddy 설치
#   2. 전용 사용자 streamapp, 폴더(/opt/streamapp, /var/lib/streamapp, /etc/streamapp) 만들기
#   3. MediaMTX v1.21.3 linux_arm64(또는 amd64) 내려받기(체크섬 확인) → /opt/mediamtx/mediamtx
#   4. 설정 깔기: Caddyfile, mediamtx.yml, systemd 유닛, streamapp.env(처음 한 번), caddy.env(도메인)
#   5. iptables 허용 규칙 추가(80·443 TCP, 8189 UDP·TCP, 1935 TCP) 후 netfilter-persistent save
#      OCI Ubuntu 이미지는 INPUT 체인 끝에 REJECT 규칙이 있어, OCI 보안 목록을 열어도 이 규칙이 없으면 막힌다.
#   6. mediamtx·caddy 시작, streamapp은 켜 두기만 한다(앱 파일은 deploy.sh가 올린다).

set -euo pipefail

MTX_VERSION="v1.21.3"
# 릴리스의 checksums.sha256 값. 버전을 바꾸면 함께 바꾼다(scripts/dev.ps1의 Windows용 값과 같은 출처).
MTX_SHA256_ARM64="27f038237cb5f77a724f6afc58eca2798f2bc4edf9989895631a80eeb41a4fe2"
MTX_SHA256_AMD64="0a2ea7eba96023631efa1bea7804823a9145b043b6646a8a05798d4ac24ed635"

APP_USER="streamapp"
APP_HOME="/var/lib/streamapp"
APP_DIR="/opt/streamapp"
CONF_DIR="/etc/streamapp"
MTX_DIR="/opt/mediamtx"
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DOMAIN=""
PUBLIC_IP=""
OPEN_RTMP=1

usage() {
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --domain) DOMAIN="${2:-}"; shift 2 ;;
        --public-ip) PUBLIC_IP="${2:-}"; shift 2 ;;
        --no-rtmp) OPEN_RTMP=0; shift ;;
        -h|--help) usage 0 ;;
        *) echo "알 수 없는 인자: $1" >&2; usage 1 ;;
    esac
done

log() { printf '\n==> %s\n' "$*"; }
die() { printf '오류: %s\n' "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "root 권한이 필요합니다: sudo bash $0 --domain <도메인>"
[[ -n "$DOMAIN" ]] || die "--domain <도메인>이 필요합니다(예: --domain stream.example.xyz)"
[[ "$DOMAIN" =~ ^[A-Za-z0-9.-]+$ ]] || die "도메인 형식이 올바르지 않습니다: $DOMAIN"
for f in Caddyfile mediamtx.yml streamapp.env.example systemd/streamapp.service systemd/mediamtx.service; do
    [[ -f "$SRC_DIR/$f" ]] || die "$SRC_DIR/$f 가 없습니다. 레포의 deploy/ 폴더를 통째로 올려 주세요"
done

case "$(uname -m)" in
    aarch64|arm64) ARCH="arm64"; MTX_SHA256="$MTX_SHA256_ARM64" ;;
    x86_64|amd64) ARCH="amd64"; MTX_SHA256="$MTX_SHA256_AMD64" ;;
    *) die "지원하지 않는 CPU 아키텍처입니다: $(uname -m)" ;;
esac
log "아키텍처: $ARCH, 도메인: $DOMAIN"

# ---- 1. 패키지 ----
log "패키지 설치"
export DEBIAN_FRONTEND=noninteractive
apt-get update -y
apt-get install -y --no-install-recommends \
    ca-certificates curl gnupg debian-keyring debian-archive-keyring apt-transport-https \
    iptables iptables-persistent netfilter-persistent tar

if ! command -v caddy >/dev/null 2>&1; then
    log "Caddy 설치(공식 apt 저장소)"
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
        | gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
        > /etc/apt/sources.list.d/caddy-stable.list
    chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
    apt-get update -y
    apt-get install -y caddy
fi
caddy version

# ---- 공인 IP ----
if [[ -z "$PUBLIC_IP" ]]; then
    # OCI VM의 네트워크 카드에는 사설 IP만 있다(공인 IP는 1:1 NAT). 바깥에서 본 주소를 묻는다.
    PUBLIC_IP="$(curl -fsS -m 10 https://checkip.amazonaws.com | tr -d '[:space:]' || true)"
fi
[[ "$PUBLIC_IP" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] \
    || die "공인 IP를 알 수 없습니다. --public-ip <OCI 콘솔의 공용 IP 주소>로 알려 주세요"
log "공인 IP: $PUBLIC_IP"

# ---- 2. 사용자·폴더 ----
log "사용자·폴더 준비"
if ! id "$APP_USER" >/dev/null 2>&1; then
    useradd --system --home-dir "$APP_HOME" --create-home --shell /usr/sbin/nologin "$APP_USER"
fi
install -d -o "$APP_USER" -g "$APP_USER" -m 750 "$APP_HOME" "$APP_HOME/data" "$APP_HOME/data/recordings"
install -d -o root -g root -m 755 "$APP_DIR" "$APP_DIR/releases" "$MTX_DIR"
install -d -o root -g "$APP_USER" -m 750 "$CONF_DIR"

# ---- 3. MediaMTX ----
if [[ ! -x "$MTX_DIR/mediamtx" ]] || ! "$MTX_DIR/mediamtx" --version 2>/dev/null | grep -q "$MTX_VERSION"; then
    log "MediaMTX $MTX_VERSION ($ARCH) 내려받기"
    tarball="mediamtx_${MTX_VERSION}_linux_${ARCH}.tar.gz"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    curl -fsSL -o "$tmp/$tarball" "https://github.com/bluenviron/mediamtx/releases/download/${MTX_VERSION}/${tarball}"
    echo "$MTX_SHA256  $tmp/$tarball" | sha256sum -c -
    tar -xzf "$tmp/$tarball" -C "$tmp" mediamtx
    install -o root -g root -m 755 "$tmp/mediamtx" "$MTX_DIR/mediamtx"
fi
"$MTX_DIR/mediamtx" --version

# ---- 4. 설정 ----
log "설정 깔기"
install -o root -g "$APP_USER" -m 640 "$SRC_DIR/mediamtx.yml" "$CONF_DIR/mediamtx.yml"
if [[ ! -f "$CONF_DIR/streamapp.env" ]]; then
    sed -e "s/__DOMAIN__/$DOMAIN/g" -e "s/__PUBLIC_IP__/$PUBLIC_IP/g" \
        "$SRC_DIR/streamapp.env.example" > "$CONF_DIR/streamapp.env"
    chown root:"$APP_USER" "$CONF_DIR/streamapp.env"
    chmod 640 "$CONF_DIR/streamapp.env"
    echo "$CONF_DIR/streamapp.env 를 만들었습니다. STREAMAPP_DEFAULT_PASSWORD와 토스 테스트 키를 채워 주세요."
else
    echo "$CONF_DIR/streamapp.env 가 이미 있어 그대로 둡니다(도메인·IP를 바꿨다면 직접 고쳐 주세요)."
fi
# Caddy에는 도메인만 넘긴다(토스 시크릿 키가 든 streamapp.env는 넘기지 않는다).
printf 'STREAMAPP_DOMAIN=%s\n' "$DOMAIN" > "$CONF_DIR/caddy.env"
chmod 644 "$CONF_DIR/caddy.env"
install -d -m 755 /etc/systemd/system/caddy.service.d
printf '%s\n' \
    '# setup-server.sh가 만든다: Caddyfile의 {$STREAMAPP_DOMAIN}을 채운다.' \
    '[Service]' \
    "EnvironmentFile=$CONF_DIR/caddy.env" \
    > /etc/systemd/system/caddy.service.d/streamapp.conf
install -o root -g root -m 644 "$SRC_DIR/Caddyfile" /etc/caddy/Caddyfile
install -o root -g root -m 644 "$SRC_DIR/systemd/streamapp.service" /etc/systemd/system/streamapp.service
install -o root -g root -m 644 "$SRC_DIR/systemd/mediamtx.service" /etc/systemd/system/mediamtx.service

# 설정 검사(MTX_ 덮어쓰기까지 적용해 본다)
(
    set -a
    # shellcheck disable=SC1091
    . "$CONF_DIR/streamapp.env"
    set +a
    "$MTX_DIR/mediamtx" --validate-conf="$CONF_DIR/mediamtx.yml"
)
STREAMAPP_DOMAIN="$DOMAIN" caddy validate --adapter caddyfile --config /etc/caddy/Caddyfile

# ---- 5. iptables(OCI Ubuntu 이미지 기본 규칙 보완) ----
log "iptables 허용 규칙"
# 규칙 하나를 INPUT 체인의 첫 REJECT 앞에 넣는다(이미 있으면 건너뛴다).
allow() {
    local proto="$1" port="$2"
    local rule=(-p "$proto" -m state --state NEW -m "$proto" --dport "$port" -j ACCEPT)
    if iptables -C INPUT "${rule[@]}" 2>/dev/null; then
        echo "  이미 있음: $proto/$port"
        return
    fi
    local pos
    pos="$(iptables -L INPUT --line-numbers -n | awk '$2 == "REJECT" { print $1; exit }')"
    if [[ -n "$pos" ]]; then
        iptables -I INPUT "$pos" "${rule[@]}"
    else
        iptables -A INPUT "${rule[@]}"
    fi
    echo "  추가: $proto/$port"
}
allow tcp 80
allow tcp 443
allow udp 8189
allow tcp 8189
if [[ $OPEN_RTMP -eq 1 ]]; then
    allow tcp 1935
fi
netfilter-persistent save

# ---- 6. 서비스 ----
log "서비스 시작"
systemctl daemon-reload
systemctl enable --now mediamtx
systemctl enable streamapp
if [[ -x "$APP_DIR/current/server" ]]; then
    systemctl restart streamapp
else
    echo "앱 파일이 아직 없어 streamapp은 켜 두기만 했습니다. 로컬에서 deploy.sh로 올리면 시작됩니다."
fi
systemctl enable caddy
systemctl restart caddy

log "완료"
echo "다음 단계:"
echo "  1. sudo nano $CONF_DIR/streamapp.env  (STREAMAPP_DEFAULT_PASSWORD, 토스 테스트 키) 후 sudo systemctl restart mediamtx streamapp"
echo "  2. 로컬에서 앱 올리기: bash deploy/deploy.sh --host ubuntu@$PUBLIC_IP --tarball <streamapp-server-*.tar.gz>"
echo "  3. 확인: curl -I https://$DOMAIN/healthz"

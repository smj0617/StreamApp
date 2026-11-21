#!/usr/bin/env bash
# StreamApp 앱 배포: 미리 만든 리눅스 서버 빌드(tar.gz)를 서버에 올리고 streamapp 서비스를 다시 시작한다.
# 로컬(Git Bash, WSL, macOS, 리눅스)이나 CI에서 실행한다. 서버는 setup-server.sh로 준비돼 있어야 한다.
#
# 사용법:
#   bash deploy/deploy.sh --host ubuntu@<공인 IP> --tarball streamapp-server-linux-arm64.tar.gz [--key ~/.ssh/oci.key] [--config]
#
#   --host     SSH 대상(sudo를 쓸 수 있는 사용자. OCI Ubuntu 이미지는 ubuntu)
#   --tarball  release-server.yml이 만든 streamapp-server-<버전>-linux-<arm64|amd64>.tar.gz
#              (GitHub Release나 Actions 아티팩트에서 받는다: gh release download <태그> -p 'streamapp-server-*-linux-arm64.tar.gz')
#   --key      SSH 개인 키(없으면 ssh 기본값)
#   --config   레포의 deploy/ 설정(Caddyfile, mediamtx.yml, systemd 유닛)도 다시 깔고 caddy·mediamtx를 다시 읽힌다.
#              streamapp.env는 건드리지 않는다.
#
# 서버에서 일어나는 일:
#   /opt/streamapp/releases/<시각>/ 에 풀기(server 실행 파일 + public/) → /opt/streamapp/current 링크 바꾸기 →
#   streamapp 다시 시작 → http://127.0.0.1:8080/healthz 가 200이 될 때까지 기다리기(30초).
#   실패하면 이전 릴리스로 링크를 되돌리고 다시 시작한다. 릴리스는 최근 5개만 남긴다.

set -euo pipefail

HOST=""
TARBALL=""
KEY=""
WITH_CONFIG=0
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --host) HOST="${2:-}"; shift 2 ;;
        --tarball) TARBALL="${2:-}"; shift 2 ;;
        --key) KEY="${2:-}"; shift 2 ;;
        --config) WITH_CONFIG=1; shift ;;
        -h|--help) usage 0 ;;
        *) echo "알 수 없는 인자: $1" >&2; usage 1 ;;
    esac
done

die() { printf '오류: %s\n' "$*" >&2; exit 1; }
[[ -n "$HOST" ]] || die "--host <사용자@서버>가 필요합니다"
[[ -n "$TARBALL" && -f "$TARBALL" ]] || die "--tarball <파일>이 필요합니다(없는 파일: '$TARBALL')"
# 빌드 묶음 안에 server 실행 파일과 public/ 폴더가 있는지 미리 본다.
listing="$(tar -tzf "$TARBALL")"
grep -qE '^([^/]+/)?server$' <<<"$listing" || die "$TARBALL 안에 server 실행 파일이 없습니다"
grep -qE '^([^/]+/)?public/' <<<"$listing" || die "$TARBALL 안에 public/ 폴더가 없습니다"

SSH_OPTS=(-o StrictHostKeyChecking=accept-new)
if [[ -n "$KEY" ]]; then
    SSH_OPTS+=(-i "$KEY")
fi

echo "==> 올리기: $TARBALL → $HOST"
scp "${SSH_OPTS[@]}" "$TARBALL" "$HOST:/tmp/streamapp-server.tar.gz"
if [[ $WITH_CONFIG -eq 1 ]]; then
    echo "==> 설정 올리기: deploy/"
    ssh "${SSH_OPTS[@]}" "$HOST" 'rm -rf ~/streamapp-deploy && mkdir -p ~/streamapp-deploy'
    scp "${SSH_OPTS[@]}" -r "$SRC_DIR/." "$HOST:streamapp-deploy/"
fi

echo "==> 서버에서 설치"
# shellcheck disable=SC2087 # 아래 스크립트는 서버에서 펼친다(작은따옴표 heredoc).
ssh "${SSH_OPTS[@]}" "$HOST" "sudo bash -s -- $WITH_CONFIG" <<'REMOTE'
set -euo pipefail
WITH_CONFIG="${1:-0}"
APP_DIR=/opt/streamapp
RELEASES="$APP_DIR/releases"
CONF_DIR=/etc/streamapp
[[ -d "$RELEASES" ]] || { echo "서버가 준비되지 않았습니다. 먼저 setup-server.sh를 실행하세요" >&2; exit 1; }

if [[ "$WITH_CONFIG" == 1 ]]; then
    SRC="$(getent passwd "${SUDO_USER:-root}" | cut -d: -f6)/streamapp-deploy"
    install -o root -g streamapp -m 640 "$SRC/mediamtx.yml" "$CONF_DIR/mediamtx.yml"
    install -o root -g root -m 644 "$SRC/Caddyfile" /etc/caddy/Caddyfile
    install -o root -g root -m 644 "$SRC/systemd/streamapp.service" /etc/systemd/system/streamapp.service
    install -o root -g root -m 644 "$SRC/systemd/mediamtx.service" /etc/systemd/system/mediamtx.service
    (set -a; . "$CONF_DIR/streamapp.env"; set +a; /opt/mediamtx/mediamtx --validate-conf="$CONF_DIR/mediamtx.yml")
    (set -a; . "$CONF_DIR/caddy.env"; set +a; caddy validate --adapter caddyfile --config /etc/caddy/Caddyfile)
    systemctl daemon-reload
    systemctl restart mediamtx
    systemctl reload caddy
    echo "설정을 다시 깔았습니다."
fi

release="$RELEASES/$(date -u +%Y%m%d%H%M%S)"
mkdir -p "$release"
# 묶음 맨 위 폴더(streamapp-server/)가 있으면 벗긴다.
if tar -tzf /tmp/streamapp-server.tar.gz | head -1 | grep -q '/'; then
    tar -xzf /tmp/streamapp-server.tar.gz -C "$release" --strip-components=1
else
    tar -xzf /tmp/streamapp-server.tar.gz -C "$release"
fi
rm -f /tmp/streamapp-server.tar.gz
chown -R root:root "$release"
chmod 755 "$release/server"

previous="$(readlink -f "$APP_DIR/current" 2>/dev/null || true)"
ln -sfn "$release" "$APP_DIR/current"
systemctl restart streamapp

healthy=0
for _ in $(seq 1 30); do
    if curl -fsS -m 2 http://127.0.0.1:8080/healthz >/dev/null 2>&1; then
        healthy=1
        break
    fi
    sleep 1
done

if [[ $healthy -eq 1 ]]; then
    echo "배포 완료: $release"
    curl -sS -m 5 http://127.0.0.1:8080/healthz; echo
    # 최근 5개 릴리스만 남긴다.
    ls -1dt "$RELEASES"/*/ | tail -n +6 | xargs -r rm -rf
else
    echo "새 릴리스가 30초 안에 /healthz에 200으로 답하지 않았습니다. 로그:" >&2
    journalctl -u streamapp -n 40 --no-pager >&2 || true
    if [[ -n "$previous" && -d "$previous" ]]; then
        echo "이전 릴리스로 되돌립니다: $previous" >&2
        ln -sfn "$previous" "$APP_DIR/current"
        systemctl restart streamapp
    fi
    exit 1
fi
REMOTE

echo "==> 끝. 바깥에서 확인: curl -I https://<도메인>/healthz"

# 배포: OCI Always Free VM + 도메인 (과제 ⑥)

README에 옮겨 넣을 배포 절차 초안이다. 처음부터 따라 하면 `https://<도메인>`에서 녹화방송·로그인·댓글·후원(토스 테스트 모드)·휴대폰 라이브 송출·시청이 되는 서버가 생긴다. 아래에서 `stream.example.xyz`는 자리 표시이고, 실제 도메인으로 바꿔 읽는다.

## 구성

```
                  ┌──────────────────────── OCI VM (Ubuntu 24.04, aarch64) ───────────────────────┐
브라우저·데스크톱 ─┼─ 443/80 TCP ─ Caddy ─┬─ /internal/*  → 403(막음)                              │
                  │  (Let's Encrypt)     ├─ /hls/*       → 127.0.0.1:8888  MediaMTX HLS(앞부분 /hls 뗌) │
                  │                      └─ 그 밖        → 127.0.0.1:8080  앱(streamapp, Dioxus 서버)  │
휴대폰(WebRTC) ───┼─ 8189 UDP·TCP ────────── MediaMTX ICE                                         │
OBS(RTMP, 선택) ──┼─ 1935 TCP ───────────── MediaMTX RTMP                                         │
                  │  서버 안에서만: MediaMTX WebRTC HTTP 127.0.0.1:8889(앱이 WHIP 중계),            │
                  │                 Control API 127.0.0.1:9997, 앱 127.0.0.1:8080(MediaMTX 인증·훅) │
                  └───────────────────────────────────────────────────────────────────────────────┘
```

| 파일 | 서버 위치 | 내용 |
|---|---|---|
| [deploy/Caddyfile](../deploy/Caddyfile) | `/etc/caddy/Caddyfile` | HTTPS, `/hls` 프록시, `/internal` 차단, 업로드 510MB, 나머지는 앱(WebSocket 포함) |
| [deploy/mediamtx.yml](../deploy/mediamtx.yml) | `/etc/streamapp/mediamtx.yml` | 리눅스용 MediaMTX 설정(HLS·WebRTC HTTP·API는 127.0.0.1만) |
| [deploy/systemd/streamapp.service](../deploy/systemd/streamapp.service) | `/etc/systemd/system/` | 앱 서비스(사용자 `streamapp`, 작업 폴더 `/var/lib/streamapp`) |
| [deploy/systemd/mediamtx.service](../deploy/systemd/mediamtx.service) | `/etc/systemd/system/` | MediaMTX 서비스 |
| [deploy/streamapp.env.example](../deploy/streamapp.env.example) | `/etc/streamapp/streamapp.env` | 환경변수(비밀 값은 서버에만) |
| [deploy/setup-server.sh](../deploy/setup-server.sh) | (서버에서 한 번 실행) | 패키지·Caddy·사용자·폴더·MediaMTX·iptables·서비스 준비 |
| [deploy/deploy.sh](../deploy/deploy.sh) | (로컬에서 실행) | 빌드 묶음 올리기, 서비스 재시작, `/healthz` 확인, 실패 시 되돌리기 |
| [.github/workflows/release-server.yml](../.github/workflows/release-server.yml) | — | linux-arm64·amd64 빌드 묶음(tar.gz) 만들기, 태그 푸시 시 Release에 첨부 |

## 1. OCI VM 만들기

1. [Oracle Cloud Free Tier](https://www.oracle.com/cloud/free/)에 가입한다. **홈 리전**은 가입 때 한 번 고르고 바꿀 수 없으며, Always Free 컴퓨트는 홈 리전에서만 만들 수 있다. 한국이면 `South Korea Central (Seoul)` 또는 `South Korea North (Chuncheon)`.
2. 콘솔 → **Compute → Instances → Create instance**.
   - **Image**: `Canonical Ubuntu 24.04`(Shape를 A1으로 고르면 aarch64 이미지가 골라진다).
   - **Shape**: `Ampere` → `VM.Standard.A1.Flex`, **OCPU 2, Memory 12 GB**.
     Always Free는 A1 Flex를 테넌시 전체에서 OCPU 4개·메모리 24GB까지(월 3,000 OCPU 시간·18,000 GB 시간) 무료로 준다. 2 OCPU/12GB 한 대는 그 절반이라 무료 한도 안이다. 콘솔의 Shape 목록에 `Always Free-eligible` 표시가 있는지 확인한다.
   - **Networking**: 새 VCN과 공용 서브넷을 만들고 **Assign a public IPv4 address**를 켠다.
   - **SSH keys**: 키를 내려받거나(`Save private key`) 내 공개 키를 붙여 넣는다.
   - **Boot volume**: 기본값(약 47GB)으로 충분하다(Always Free 블록 스토리지 총 200GB 안).
3. 만든 뒤 인스턴스 상세의 **Public IP address**를 적어 둔다(아래 `<공인 IP>`).

**A1 재고가 없을 때**("Out of capacity for shape VM.Standard.A1.Flex"): 다른 가용성 도메인(AD)을 고르거나, 시간을 두고 다시 시도하거나, OCPU·메모리를 1/6GB로 줄여 본다. 그래도 안 되면 `VM.Standard.E2.1.Micro`(AMD x86_64, 1/8 OCPU, 메모리 1GB, Always Free로 2대까지)로 만든다. 이때는 **linux-amd64** 묶음을 쓴다(setup-server.sh는 아키텍처를 보고 MediaMTX amd64를 받는다). 메모리가 1GB라 서버에서 빌드하지 말고 CI 빌드 묶음만 올린다. 녹화방송 업로드는 디스크에 바로 쓰므로 메모리 걱정은 적지만, 동시 시청자가 많으면 느릴 수 있다.

## 2. 보안 목록(Ingress) 규칙

콘솔 → **Networking → Virtual cloud networks → (VCN) → Security Lists → Default Security List → Add Ingress Rules**. 모두 Stateless 끔(기본), Source CIDR `0.0.0.0/0`.

| 프로토콜 | 대상 포트 | 용도 |
|---|---|---|
| TCP | 22 | SSH(기본으로 있음) |
| TCP | 80 | HTTP → HTTPS 넘김, Let's Encrypt 인증 |
| TCP | 443 | HTTPS(앱, HLS 시청) |
| UDP | 8189 | WebRTC ICE(휴대폰 송출 영상·소리) |
| TCP | 8189 | WebRTC ICE(UDP가 막힌 망) |
| TCP | 1935 | RTMP(OBS 송출, 선택) |

8888(HLS), 8889(WebRTC HTTP), 9997(Control API), 8080(앱)은 **열지 않는다**. 이 포트들은 127.0.0.1에서만 듣는다.

OCI의 Ubuntu 이미지는 보안 목록과 별개로 **VM 안의 iptables**가 INPUT 체인 끝에서 모두 REJECT한다. setup-server.sh가 위 포트의 허용 규칙을 REJECT 앞에 넣고 `netfilter-persistent save`로 저장한다. 보안 목록만 열고 iptables를 열지 않으면 바깥에서 접속이 안 된다(반대도 마찬가지).

## 3. DNS A 레코드

도메인 관리 화면에서 A 레코드를 만든다.

| 이름 | 종류 | 값 | TTL |
|---|---|---|---|
| `stream`(또는 `@`) | A | `<공인 IP>` | 300 |

확인: `nslookup stream.example.xyz`가 `<공인 IP>`를 돌려줘야 한다. Caddy는 첫 시작 때 Let's Encrypt 인증서를 받는데, 이때 DNS가 이 서버를 가리키고 80·443이 열려 있어야 한다. Cloudflare 같은 프록시(주황 구름)는 끈다(DNS only). 켜면 WebRTC(8189)·RTMP(1935)가 프록시를 거치지 못한다.

## 4. 서버 준비(한 번)

로컬(레포 루트, Git Bash 또는 PowerShell의 OpenSSH)에서:

```bash
# Windows에서 받은 키 파일은 권한을 좁혀야 ssh가 쓴다(PowerShell: icacls <키> /inheritance:r /grant:r "$env:USERNAME:R").
scp -i ~/.ssh/oci.key -r deploy ubuntu@<공인 IP>:~/streamapp-deploy
ssh -i ~/.ssh/oci.key ubuntu@<공인 IP>
```

서버에서:

```bash
sudo bash ~/streamapp-deploy/setup-server.sh --domain stream.example.xyz
# 공인 IP를 못 찾으면: --public-ip <공인 IP>, OBS(1935)를 열지 않으려면: --no-rtmp
```

setup-server.sh가 하는 일: 패키지와 Caddy(공식 apt 저장소) 설치 → 사용자 `streamapp`과 `/opt/streamapp`, `/var/lib/streamapp/data`, `/etc/streamapp` 만들기 → MediaMTX v1.21.3 `linux_arm64`(또는 amd64)를 받아 체크섬 확인 후 `/opt/mediamtx/mediamtx`에 두기 → 설정 파일 깔기(`streamapp.env`는 처음 한 번만, 도메인·공인 IP를 채워서) → MediaMTX·Caddy 설정 검사 → iptables 열기 → mediamtx·caddy 시작, streamapp 켜 두기.

그다음 비밀 값을 채운다.

```bash
sudo nano /etc/streamapp/streamapp.env
#   STREAMAPP_DEFAULT_PASSWORD=<기본 사용자 비밀번호. 공개 서버이므로 반드시 바꾼다>
#   STREAMAPP_TOSS_CLIENT_KEY=test_ck_...   (후원을 켤 때만, 토스 테스트 키)
#   STREAMAPP_TOSS_SECRET_KEY=test_sk_...
sudo systemctl restart mediamtx
```

Windows에서 레포를 체크아웃했다면 `deploy/` 파일은 `.gitattributes`로 LF 줄 끝을 유지한다. 직접 만든 파일을 올릴 때는 CRLF가 섞이지 않게 한다.

## 5. 빌드 묶음 만들기

로컬 Windows에서는 리눅스 빌드를 하지 않는다. GitHub Actions의 [release-server.yml](../.github/workflows/release-server.yml)이 만든다.

- 태그를 푸시하면(`git tag v0.6.0 && git push origin v0.6.0`) Release에 `streamapp-server-v0.6.0-linux-arm64.tar.gz`(와 amd64)가 붙는다.
- 태그 없이 만들려면 Actions 탭 → **Release server** → **Run workflow**. 실행 결과의 Artifacts에서 받는다.

빌드 명령은 `dx bundle --web --release --package web --out-dir bundle-out`(dx 0.7.10)이다. 풀스택 앱이라 서버 실행 파일 `bundle-out/server`(패키지 이름 `web`이 아니라 dx가 붙이는 `server`)와 화면 파일 `bundle-out/public/`이 함께 생긴다. 서버는 실행 파일 옆의 `public/`을 내준다(바꾸려면 `DIOXUS_PUBLIC_PATH`). 묶음 안은 `streamapp-server/server`, `streamapp-server/public/`이다.

```bash
gh release download v0.6.0 -p 'streamapp-server-*-linux-arm64.tar.gz'
# 또는 Actions 실행 결과: gh run download <실행 ID> -n streamapp-server-linux-arm64
```

## 6. 앱 올리기(배포할 때마다)

로컬(Git Bash, WSL, macOS, 리눅스) 레포 루트에서:

```bash
bash deploy/deploy.sh --host ubuntu@<공인 IP> --key ~/.ssh/oci.key \
  --tarball streamapp-server-v0.6.0-linux-arm64.tar.gz
# deploy/ 설정(Caddyfile, mediamtx.yml, systemd 유닛)을 바꿨으면 --config를 더한다(streamapp.env는 건드리지 않는다).
```

deploy.sh는 묶음을 `/opt/streamapp/releases/<시각>/`에 풀고 `/opt/streamapp/current` 링크를 바꾼 뒤 `streamapp`을 다시 시작한다. 30초 안에 `http://127.0.0.1:8080/healthz`가 200이 되지 않으면 로그를 보여 주고 이전 릴리스로 되돌린다. 릴리스는 최근 5개만 남긴다. DB(`/var/lib/streamapp/data/streamapp.db`)와 녹화방송 영상(`/var/lib/streamapp/data/recordings/`)은 릴리스와 따로 있어 배포해도 남는다. 첫 시작 때 빈 DB에 마이그레이션과 기본 사용자(철수·영희·민수, 비밀번호는 `STREAMAPP_DEFAULT_PASSWORD`)가 생긴다.

## 7. 환경변수

`/etc/streamapp/streamapp.env`(템플릿 [deploy/streamapp.env.example](../deploy/streamapp.env.example)). `streamapp.service`와 `mediamtx.service`가 함께 읽는다. 값을 바꾸면 `sudo systemctl restart mediamtx streamapp`.

| 이름 | 배포 값 | 기본값(없을 때) | 읽는 쪽 |
|---|---|---|---|
| `IP` | `127.0.0.1` | `127.0.0.1` | 앱(dioxus 서버가 듣는 IP) |
| `PORT` | `8080` | `8080` | 앱(듣는 포트. 바꾸면 Caddyfile·mediamtx.yml도 바꾼다) |
| `STREAMAPP_DB` | `/var/lib/streamapp/data/streamapp.db` | `data/streamapp.db` | 앱 |
| `STREAMAPP_RECORDINGS_DIR` | `/var/lib/streamapp/data/recordings` | `data/recordings` | 앱 |
| `STREAMAPP_HLS_BASE_URL` | `https://<도메인>/hls` | `http://localhost:8888` | 앱(라이브 재생 주소 앞부분) |
| `STREAMAPP_OBS_SERVER` | `rtmp://<도메인>/` | `rtmp://localhost/` | 앱(내 채널 설정의 OBS 서버 칸) |
| `STREAMAPP_MEDIAMTX_API` | `http://127.0.0.1:9997` | 같음 | 앱(시작 시 송출 상태 맞추기, `/healthz`) |
| `STREAMAPP_MEDIAMTX_WEBRTC` | `http://127.0.0.1:8889` | 같음 | 앱(휴대폰 방송 WHIP 중계) |
| `STREAMAPP_DEFAULT_PASSWORD` | 직접 정한 비밀번호 | `test1234` | 앱(빈 DB의 기본 사용자) |
| `STREAMAPP_TOSS_CLIENT_KEY`, `STREAMAPP_TOSS_SECRET_KEY` | 토스 테스트 키(선택) | 없음 → 후원 결제 끔 | 앱 |
| `MTX_HLSALLOWORIGINS` | `https://<도메인>` | 설정 파일 값 | MediaMTX |
| `MTX_WEBRTCADDITIONALHOSTS` | `<공인 IP>` | `[]` | MediaMTX(ICE 후보로 알릴 공인 IP) |
| `STREAMAPP_DOMAIN` | `<도메인>` | — | Caddy(`/etc/streamapp/caddy.env`, setup-server.sh가 만든다) |

토스 후원을 켰다면 토스 개발자센터의 웹훅 주소를 `https://<도메인>/api/payments/toss/webhook`으로 등록한다. 결제창이 돌아오는 주소(`/donations/success`, `/donations/fail`)는 브라우저의 현재 출처로 만들어서 따로 설정할 것이 없다.

## 8. 확인

```bash
# HTTPS와 상태 확인(200, JSON에 "database":"ok", "mediamtx":"ok")
curl -I https://stream.example.xyz/healthz
curl https://stream.example.xyz/healthz
# {"status":"ok","database":"ok","mediamtx":"ok","version":"0.1.0"}

# 내부 경로는 바깥에서 막힌다(403)
curl -i -X POST https://stream.example.xyz/internal/mediamtx/auth

# 바깥에 열린 포트만 듣는지(서버에서): 8888·8889·9997·8080은 127.0.0.1이어야 한다
sudo ss -tulpn | grep -E 'caddy|mediamtx|web'

# 로그
journalctl -u streamapp -f
journalctl -u mediamtx -f
journalctl -u caddy -f
```

`/healthz`는 DB에 `SELECT 1`이 되면 200, 안 되면 503이다. MediaMTX가 꺼져 있으면 `"mediamtx":"unreachable"`로 알려 주지만 503은 아니다(녹화방송·로그인·댓글은 MediaMTX 없이도 된다).

### 휴대폰 송출 확인

1. PC 브라우저에서 `https://stream.example.xyz`를 열고 회원가입하거나 기본 사용자로 로그인한다.
2. 휴대폰(와이파이를 끄고 LTE/5G로 해 보면 바깥망에서도 되는지 확인된다) 브라우저에서 `https://stream.example.xyz/broadcast`를 열고 같은 사용자로 로그인한 뒤 **방송 시작**을 누른다. HTTPS라서 카메라 권한을 물어본다.
   - 휴대폰은 `https://<도메인>/api/broadcast/whip`으로 앱에 WHIP 요청을 보내고, 앱이 채널 ID와 송출 키를 붙여 `127.0.0.1:8889`의 MediaMTX로 넘긴다. 영상·소리는 휴대폰이 `<공인 IP>:8189`(UDP, 막히면 TCP)로 직접 보낸다.
3. PC 홈에 라이브가 뜨고, 시청 페이지(`/live/<채널ID>`)에서 영상이 재생되는지 본다. 재생 주소는 `https://<도메인>/hls/<채널ID>/index.m3u8`이다.
4. 안 될 때:
   - `journalctl -u mediamtx`에 `[WebRTC] [session ...] created`만 있고 `is publishing`이 없으면 ICE가 닿지 않은 것이다. 보안 목록과 iptables의 8189 UDP·TCP, `MTX_WEBRTCADDITIONALHOSTS`가 공인 IP인지 확인한다.
   - 라이브가 홈에 안 뜨면 훅이 앱에 닿지 않은 것이다. `journalctl -u streamapp`에서 "라이브 시작" 로그를 찾는다.
   - HLS 404면 `curl -I https://stream.example.xyz/hls/<채널ID>/index.m3u8`로 Caddy 프록시를 확인한다.

### OBS 송출 확인(1935를 열었을 때)

내 채널 설정 화면의 OBS 서버(`rtmp://stream.example.xyz/`)와 스트림 키(`<채널ID>?key=<송출 키>`)를 OBS에 넣고 방송 시작. RTMP는 암호화되지 않는다(송출 키가 평문으로 간다).

## 데스크톱 클라이언트와 재생 주소

라이브 재생 주소는 서버가 API 응답에 절대 주소로 싣는다. 클라이언트는 따로 주소를 조합하지 않는다.

- `GET /api/live-channels` → `channels[].playback_url`
- `GET /api/watch/{channel_id}` → `playback_url`(라이브 중이 아니어도 채널의 재생 주소가 온다. `live`가 `null`이면 오프라인)
- `GET /api/recordings/{id}` → `video_url`(녹화방송 mp4·webm. `/media/recordings/<파일>`처럼 **앱 출처 기준 상대 주소**라, 데스크톱은 서버 주소를 앞에 붙인다). `GET /api/recordings` 목록에는 영상 주소가 없다(상세에서 받는다).

## 알려진 한계

- 세션 쿠키에 `Secure` 속성이 없다(HTTPS로만 서비스하므로 실제로는 암호화되지만, 붙이려면 로컬 http 개발과 나눠야 한다).
- RTMP(1935)는 평문이다. OBS 송출 키가 노출될 수 있으니 필요 없으면 `--no-rtmp`로 닫는다.
- 앱·MediaMTX 한 대 구성이라 서버를 다시 시작하는 동안 라이브가 끊긴다. 앱이 다시 뜨면 Control API로 송출 상태를 맞춘다.
- 백업은 없다. `/var/lib/streamapp/data`를 주기적으로 복사하거나 OCI 부트 볼륨 백업(Always Free 5개)을 쓴다.

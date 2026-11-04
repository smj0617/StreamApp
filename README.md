# StreamApp

치지직을 본뜬 라이브 방송 플랫폼. Dioxus 0.7.10 풀스택(웹)과 Cargo workspace 모노레포로 만들고, 라이브 영상은 외부 미디어 서버 MediaMTX가 맡는다. 작게 만들고 반복 개선한다.

용어는 [GLOSSARY.md](GLOSSARY.md), 설계 결정은 [docs/adr/](docs/adr/)에 있다.

## 과제: 치지직 만들기

과제의 각 단계는 앞 단계 기능을 유지한 채 이 저장소에서 이어서 확장한다. 실행 방법은 [로컬에서 실행하기](#로컬에서-실행하기-windows), 환경변수는 [환경변수](#환경변수)에 있다.

### ① 인증·인가 없는 녹화방송

**구현 범위**

- **녹화방송 등록** `/recordings/new`: 제목(1~100자)과 영상 파일(mp4·webm, 최대 500MB)을 골라 올린다. 브라우저가 파일을 멀티파트로 `POST /api/recordings/upload`에 보내고, 서버는 받은 조각을 바로 `data/recordings/<무작위 이름>.<확장자>`에 쓴다. 제목·확장자·크기를 서버에서 확인하며, 거절되거나 중간에 끊기면 파일을 남기지 않는다.
- **녹화방송 목록**: 홈의 "녹화방송" 칸에 최근 등록순 카드(제목, 채널, 등록 후 지난 시간)로 보인다.
- **녹화방송 상세** `/recordings/<번호>`: `<video>`로 재생한다. 앱이 `/media/recordings/<파일 이름>`으로 영상 파일을 내주고, Range 요청을 지원해 되감기·건너뛰기가 된다. 없는 번호면 "찾을 수 없습니다"를 보여 준다.
- **영속 저장**: 제목, 채널, 영상 위치(파일 이름), 등록 시각을 SQLite(`data/streamapp.db`, 마이그레이션 [0003](packages/api/migrations/0003_recordings.sql))에 저장한다. 영상 파일은 `data/recordings/`에 있어 앱을 다시 켜도 그대로 보인다([ADR 0003](docs/adr/0003-app-stores-recording-files.md)).
- **로그인 없음**: 회원가입·로그인·비밀번호가 없다. 등록할 때는 상단 메뉴에서 닉네임으로 사용자를 고르기만 하면 되고(녹화방송이 그 사용자의 채널에 붙는다), 목록·상세·재생은 사용자를 고르지 않아도 된다([ADR 0001](docs/adr/0001-start-without-auth.md)).
- 이번 단계에서 하지 않은 것: 인증·인가, 녹화방송 수정·삭제, 썸네일, 댓글, 도네이션. 라이브 송출·시청·채팅은 이 단계 전에 만들어 둔 기능이다.

**직접 확인 절차**

1. `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Port 8090`로 앱을 띄우고 `http://localhost:8090`을 연다. 홈의 "녹화방송" 칸에 "아직 등록된 녹화방송이 없습니다"가 보인다.
2. 상단 "녹화방송 등록"을 누른다. 사용자를 고르지 않았으면 "사용자를 먼저 골라 주세요"가 보인다. 오른쪽 위 메뉴에서 `철수`를 고른다.
3. 시험용 영상이 없으면 만든다: `ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30:duration=12 -f lavfi -i sine=frequency=440:duration=12 -c:v libx264 -pix_fmt yuv420p -c:a aac -movflags +faststart sample.mp4`
4. 제목 없이, 또는 파일 없이 "등록"을 누르면 거절 사유가 보인다. `.avi` 같은 다른 형식도 거절된다.
5. 제목을 적고 `sample.mp4`를 고른 뒤 "등록"을 누른다. 진행률이 보이다가 상세 페이지 `/recordings/1`로 옮겨 가고, 영상이 재생되며 막대로 건너뛸 수 있다.
6. 홈으로 가면 "녹화방송" 칸에 방금 올린 카드가 있고, 누르면 상세 페이지로 간다.
7. 앱을 끄고(Ctrl+C) 같은 명령으로 다시 켠다. 홈 목록과 `/recordings/1` 재생이 그대로 된다.
8. `/recordings/999`처럼 없는 번호를 열면 "녹화방송을 찾을 수 없습니다"가 보인다.

자동 테스트: 앱 서비스 테스트 [tests/recordings.rs](packages/api/tests/recordings.rs)가 등록·목록·상세·거절 규칙·재시작 뒤 조회를, HTTP 테스트 [tests/recording_http.rs](packages/api/tests/recording_http.rs)가 멀티파트 올리기·거절 응답·영상 파일 내주기(Range 206)를 확인한다.

### ② 인증·인가 없는 휴대폰 라이브 송출

**구현 범위**

- **휴대폰 방송** `/broadcast`: 휴대폰 브라우저에서 "카메라 켜기"를 누르면 카메라·마이크 권한을 묻고(`getUserMedia`), 미리보기가 나온다. 전면·후면 카메라를 고를 수 있고, 라이브 제목을 적을 수 있다. "방송 시작"을 누르면 WebRTC로 송출을 시작하고, "방송 종료"를 누르면 연결을 닫고 카메라를 끈다. 페이지를 떠나도 송출이 끝난다. 화면은 `카메라 꺼짐 → 미리보기 중 → 연결 중… → 방송 중(LIVE)` 단계를 보여 준다. 실패하면 이유(권한 거부, HTTPS 아님, 이미 송출 중, 미디어 서버 꺼짐, 영상 연결 실패)를 알려 준다.
- **송출 경로**([ADR 0004](docs/adr/0004-phone-broadcast-via-whip-relay.md)): 휴대폰은 송출 시작 요청(WHIP, SDP 제안)을 앱의 `POST /api/broadcast/whip`으로 보낸다. 앱은 현재 사용자의 채널 ID와 송출 키를 붙여 MediaMTX(`http://127.0.0.1:8889/<채널ID>/whip?key=…`)로 넘긴다. 송출 키는 휴대폰으로 가지 않는다. 종료는 `DELETE /api/broadcast/whip/<세션>`이다. 영상(H.264)과 소리(Opus)는 휴대폰이 PC의 MediaMTX(ICE 8189)로 직접 보낸다.
- **겹치는 송출 거절**: 이미 라이브 중인 채널(다른 휴대폰·OBS)로 방송을 시작하면 앱이 MediaMTX에 넘기기 전에 409 "이 채널은 이미 방송 중입니다"로 거절한다. MediaMTX도 `overridePublisher: false`로 두어, 나중에 온 송출이 방송 중인 송출을 밀어내지 못한다.
- **방송 상태**: 송출이 시작되면 MediaMTX가 송출 키를 앱에 확인하고, 훅이 라이브를 연다. 끊기면 라이브를 닫는다. OBS 송출과 같은 경로라서 홈의 라이브 카드(LIVE, 경과 시간)와 시청 페이지 `/live/<채널ID>`(LIVE / 라이브 종료 / 오프라인)가 그대로 동작한다.
- **시청**: 다른 화면(PC 브라우저)에서 홈의 라이브 카드를 눌러 시청한다(HLS). 방송을 끝내면 시청 페이지에 바로 "라이브가 종료되었습니다"가 뜬다.
- **로그인 없음**: 휴대폰에서도 닉네임으로 사용자를 고르기만 하면 그 사용자의 채널로 방송한다. 시청은 사용자를 고르지 않아도 된다.
- 1단계 녹화방송(등록·목록·상세·재생)은 그대로 있다.
- 이번 단계에서 하지 않은 것: 인증·인가, 휴대폰에서의 시청(시청용 HLS 주소가 PC의 `localhost:8888`이다), 다른 망(LTE 등)에서의 송출(TURN 서버 없음), 화질 선택.

**접속 환경과 권한**

- **같은 와이파이**: 휴대폰과 PC가 같은 공유기(와이파이)에 있어야 한다. 영상이 휴대폰에서 PC의 사설 IP로 직접 간다.
- **HTTPS**: 휴대폰 브라우저는 HTTPS 주소에서만 카메라를 열어 준다. PC에서 앱을 띄운 뒤 터널로 HTTPS 주소를 만든다. 처음 한 번 `winget install --id Cloudflare.cloudflared`로 설치한다. 계정은 필요 없다.

  ```powershell
  cloudflared tunnel --url http://127.0.0.1:8090
  ```

  설치 직후에는 새 PowerShell 창을 열어야 `cloudflared`가 PATH에 잡힌다. MSYS2·Git Bash처럼 Windows PATH를 물려받지 않는 셸에서는 전체 경로 `"/c/Program Files (x86)/cloudflared/cloudflared.exe"`로 실행한다. 출력에 나오는 `https://<무작위>.trycloudflare.com` 주소를 휴대폰에서 연다. 터널은 송출 시작 요청과 화면만 나르고, 영상은 와이파이로 바로 간다.
- **방화벽**: Windows 방화벽이 `mediamtx.exe`를 물으면 "개인 네트워크"를 허용한다. 막았다면 Windows 보안 → 방화벽 → 앱 허용에서 `mediamtx.exe`를 켠다. WebRTC 8889(TCP)와 ICE 8189(UDP·TCP)를 쓴다.
- **카메라·마이크 권한**: "카메라 켜기"를 누르면 브라우저가 묻는다. 허용한다. 거부했다면 주소창 왼쪽의 사이트 설정에서 카메라·마이크를 "허용"으로 바꾸고 다시 누른다. iOS Safari는 설정 → Safari → 카메라·마이크에서도 확인한다.
- **브라우저**: H.264로 보낼 수 있는 최신 Chrome(Android)이나 Safari(iOS). 방송 중에는 화면을 끄거나 다른 앱으로 가지 않는다(브라우저가 카메라를 멈춘다).

**직접 확인 절차(송출 시작·종료 확인)**

1. PC에서 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Port 8090`로 앱과 MediaMTX를 띄운다. 다른 창에서 `cloudflared tunnel --url http://127.0.0.1:8090`을 실행해 HTTPS 주소를 얻는다.
2. 휴대폰을 PC와 같은 와이파이에 연결하고 그 HTTPS 주소를 연다. 상단 메뉴에서 사용자(예: `철수`)를 고르고 "휴대폰 방송"으로 간다. 사용자를 고르지 않았으면 "사용자를 먼저 골라 주세요"가 보인다.
3. "카메라 켜기" → 권한 허용 → 미리보기가 나온다. 라이브 제목을 적고 "방송 시작"을 누른다. 상태가 "연결 중…"에서 "방송 중"으로 바뀌고 미리보기에 LIVE가 붙는다.
4. **송출 시작 확인**: PC 브라우저에서 `http://localhost:8090`을 연다(다른 화면). 홈 "라이브" 칸에 적은 제목의 LIVE 카드가 있다. 누르면 휴대폰 카메라 영상과 소리가 나온다(처음엔 소리가 꺼져 있으니 플레이어에서 켠다). PC 쪽 `dev.ps1` 로그에 `휴대폰 방송 연결`과 `라이브 시작`이 찍힌다.
5. **송출 종료 확인**: 휴대폰에서 "방송 종료"를 누른다. PC의 시청 페이지에 바로 "라이브가 종료되었습니다"와 "종료" 표시가 뜨고, 홈을 새로고침하면 라이브 카드가 사라진다. 로그에 `휴대폰 방송 종료`와 `라이브 종료`가 찍힌다.
6. 실패 확인: 권한을 거부하면 권한 안내가, OBS나 다른 휴대폰으로 같은 채널을 송출 중이면 "이 채널은 이미 방송 중입니다"가(방송 중인 쪽은 끊기지 않는다), 터널 없이 `http://` 주소로 열면 HTTPS 안내가 보인다.
7. 1단계 확인: 홈의 "녹화방송" 칸과 `/recordings/<번호>` 재생이 그대로 된다.

PC만으로 미리 확인하려면 PC 브라우저에서 `http://localhost:8090/broadcast`를 연다(localhost는 HTTPS 없이도 카메라를 연다). 웹캠이 있으면 같은 절차로 송출된다.

자동 테스트: [tests/whip_http.rs](packages/api/tests/whip_http.rs)가 가짜 MediaMTX를 띄우고 확인한다. 송출 시작 요청이 현재 사용자 채널과 송출 키를 붙여 넘어가는지, 응답의 세션 주소에서 송출 키가 빠지는지, 종료 요청, 사용자 없음·SDP 아님·이미 라이브 중(409)·MediaMTX 거절(409)·MediaMTX 꺼짐(502)·잘못된 세션을 본다. 송출 키 확인과 라이브 시작·종료는 [tests/mediamtx_http.rs](packages/api/tests/mediamtx_http.rs)·[tests/service.rs](packages/api/tests/service.rs)가 확인한다.

## 현황 (2026-11-04)

### 과제 ②: 휴대폰 라이브 송출 — 구현

- 자동 테스트 117개 통과(휴대폰 방송 중계 8개 추가), clippy·fmt·wasm 빌드 통과.
- 실제 휴대폰(같은 와이파이, cloudflared 터널)으로 방송 시작 → PC 시청 페이지에서 세로 720×1280 영상·소리 재생 → 방송 종료까지 확인했다. 방송 중 전면·후면 카메라 바꾸기(연결 유지)는 PC에서 가상 카메라로 확인했다.
- RTMP로 방송 중인 채널에 휴대폰 방송을 시작하면 409로 거절되고 기존 방송은 끊기지 않는 것을 확인했다.
- PC Chrome에서 확인: 카메라 대신 캔버스·사인파로 만든 가상 스트림으로 `/broadcast`에서 카메라 켜기 → 방송 시작 → `방송 중`(H.264·Opus) → 다른 탭 홈에 LIVE 카드 → 시청 페이지에서 영상·소리 재생 → 방송 종료 → 시청 페이지에 즉시 "라이브가 종료되었습니다" → 녹화방송 목록 유지. 휴대폰 크기(375px)에서 화면 배치와 사용자 메뉴도 확인했다.
- 확인하지 못한 것: 브라우저별 차이(한 대의 휴대폰으로만 확인), 실제 휴대폰에서의 방송 중 카메라 바꾸기.

### 과제 ①: 녹화방송 — 완료

- 자동 테스트 109개 통과(녹화방송 23개 추가), clippy·fmt·wasm 빌드 통과.
- 브라우저에서 확인: 사용자 고르기 전 안내 → 파일 없이 등록 거절 → 4.6MB mp4 등록 → 상세 페이지 자동 이동·재생·8초 지점으로 건너뛰기(Range 206) → 홈 카드 → `.avi` 거절 → 없는 번호 안내 → 앱 재시작 뒤 같은 녹화방송 재생.
- 확인하지 못한 것: 수백 MB 크기 파일 올리기, 화면에서 빈 제목으로 올리기(HTTP 테스트로만 확인), webm 파일의 브라우저 재생(자동 테스트로 등록·내주기만 확인).

### 첫 반복: 송출·시청·채팅 — 완료

스펙 [#2](https://github.com/smj0617/StreamApp/issues/2), 티켓 #3~#11, PR [#12](https://github.com/smj0617/StreamApp/pull/12)로 `main`에 머지했다.

- **사용자 고르기**: 비밀번호 없이 닉네임만으로 사용자를 만들거나 고른다(인증 없음, [ADR 0001](docs/adr/0001-start-without-auth.md)). 사용자마다 채널이 하나씩 자동으로 생긴다.
- **송출**: OBS나 테스트 스크립트로 송출하면 라이브가 시작되고, 멈추면 끝난다. 송출 키는 앱이 확인한다([ADR 0002](docs/adr/0002-mediamtx-handles-video.md)).
- **홈**: 라이브 중인 채널 카드 목록(최근 시작순, 경과 시간).
- **시청 페이지** `/live/<채널ID>`: hls.js로 저지연 HLS 재생, 오프라인·없는 채널·라이브 종료 표시.
- **채팅**: 라이브 중에만 열리는 실시간 채팅(WebSocket, 저장하지 않음). 사용자를 골라야 보낼 수 있다. 라이브 제목이 바뀌면 시청 페이지에 바로 반영된다.
- **내 채널 설정**: OBS "서버"·"스트림 키" 값 복사, 다음 라이브 제목, 라이브 중 제목 변경.
- **앱 재시작 대비**: 앱이 시작할 때 MediaMTX Control API로 송출 상태를 맞춘다.

확인한 것:

- 자동 테스트 86개 통과(앱 서비스 + 메모리 SQLite, MediaMTX 연동 HTTP), clippy·fmt·wasm 빌드 통과.
- 실제 OBS 송출 → 홈 카드 → 시청 페이지 재생 → 두 사용자 간 채팅 → 방송 중단 시 종료 표시까지 브라우저에서 확인.

확인하지 못한 것:

- 내 채널 설정의 "복사" 버튼을 브라우저에서 직접 눌러 보지 않았다.

### 다음 후보

- [#13](https://github.com/smj0617/StreamApp/issues/13) 리팩터링: 데이터 뭉치 묶기, 채널 ID·송출 키 전용 타입
- 첫 반복에서 범위 밖으로 둔 것: 다시보기(VOD), 클립, 치즈(후원), 구독, 팔로우, 검색, 카테고리, 시청자 수, 썸네일, 화질 선택, 송출 키 재발급, 끊긴 송출을 같은 라이브로 잇는 유예 시간, 홈 목록 실시간 갱신, 채팅 기록·도배 방지, 인증

## 구조

```text
packages/
├── api/                 # 서버 함수, 앱 서비스(Dioxus 비의존), MediaMTX 연동·녹화방송 파일·휴대폰 방송 중계 axum Router
│   ├── migrations/      # 0001 사용자·채널, 0002 라이브, 0003 녹화방송 (기존 파일은 수정하지 말고 새 번호로 추가)
│   └── tests/           # 앱 서비스(메모리 SQLite), MediaMTX 연동·녹화방송 파일·휴대폰 방송 중계 HTTP 테스트
└── web/                 # 화면: 홈, 시청 페이지, 휴대폰 방송, 녹화방송 등록·상세, 내 채널 설정, 사용자 메뉴
scripts/                 # dev.ps1, test-publish.ps1
tools/mediamtx/          # MediaMTX v1.21.3 설정 (실행 파일은 dev.ps1이 .tools\로 내려받음)
```

## 로컬에서 실행하기 (Windows)

필요한 도구: Visual Studio Build Tools(C++), Rust(stable, MSVC) + `wasm32-unknown-unknown` 타깃, `dx` 0.7.10, ffmpeg. `dev.ps1`이 시작할 때 확인하고 없으면 무엇을 설치할지 알려 준다.

```powershell
# 앱 + MediaMTX 실행 (기본 포트 8080, 이 PC에서는 다른 프로그램이 8080을 써서 8090으로 띄운다)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Port 8090
```

- 브라우저에서 `http://localhost:<포트>`를 연다. 오른쪽 위 메뉴에서 사용자를 고른다(빈 DB면 철수·영희·민수가 미리 만들어진다).
- **휴대폰 송출**: 위 "② 인증·인가 없는 휴대폰 라이브 송출"의 접속 환경과 절차를 따른다.
- **OBS 송출**: 내 채널 설정의 "서버"·"스트림 키" 값을 OBS의 설정 → 방송 → 서비스 "사용자 지정"에 넣고 방송을 시작한다. OBS 없이 테스트하려면:

  ```powershell
  .\scripts\test-publish.ps1 -ChannelId <채널 ID> -StreamKey <송출 키>
  ```

- **두 사용자로 채팅 확인**: 사용자 선택은 쿠키라서, 한 브라우저에서는 `localhost`와 `127.0.0.1`로 주소를 나눠 열면 서로 다른 사용자가 된다.
- **초기화**: 앱을 끄고 `data/` 폴더(`streamapp.db`와 `recordings/`)를 지우면 기본 사용자 3명만 있는 상태로 돌아간다.

### 환경변수

모두 선택 사항이며, 비밀 값은 없다. 정하지 않으면 아래 기본값을 쓴다(경로는 앱을 실행한 폴더 기준, `dev.ps1`은 레포 루트).

```powershell
$env:STREAMAPP_DB = "data/streamapp.db"                  # SQLite DB 파일
$env:STREAMAPP_RECORDINGS_DIR = "data/recordings"        # 녹화방송 영상 파일 폴더
$env:STREAMAPP_MEDIAMTX_API = "http://127.0.0.1:9997"    # MediaMTX Control API(앱 시작 시 송출 상태 맞추기)
```

## 테스트

PR과 `main` 푸시마다 GitHub Actions([.github/workflows/ci.yml](.github/workflows/ci.yml))가 아래 네 가지를 돌린다. Rust 버전은 [rust-toolchain.toml](rust-toolchain.toml)로 로컬과 CI가 같게 고정한다.

```powershell
cargo test --workspace --features api/server,web/server
cargo check -p web --target wasm32-unknown-unknown --no-default-features --features web
cargo clippy --workspace --all-targets --features api/server,web/server -- -D warnings
cargo fmt --all --check
```

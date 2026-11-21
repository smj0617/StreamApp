# StreamApp

치지직을 본뜬 라이브 방송 플랫폼. Dioxus 0.7.10 풀스택(웹)과 Cargo workspace 모노레포로 만들고, 라이브 영상은 외부 미디어 서버 MediaMTX가 맡는다. 작게 만들고 반복 개선한다.

용어는 [GLOSSARY.md](GLOSSARY.md), 설계 결정은 [docs/adr/](docs/adr/)에 있다.

## 과제: 치지직 만들기

과제의 각 단계는 앞 단계 기능을 유지한 채 이 저장소에서 이어서 확장한다(①→②→③→④→⑤→⑥). 배포한 서비스는 **https://smj0617.com**, 데스크톱 앱은 **https://smj0617.com/download**에 있다(아래 "⑥ 도메인·OCI 배포와 Windows/macOS 클라이언트"). 실행 방법은 [로컬에서 실행하기](#로컬에서-실행하기-windows), 환경변수는 [환경변수](#환경변수)에 있다.

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

### ③ 인증·인가와 댓글

이 단계부터 ①·②의 "상단 메뉴에서 닉네임으로 사용자 고르기"는 회원가입·로그인으로 바뀌었다. 녹화방송 등록·재생, 휴대폰 송출·시청 기능은 그대로이고, 로그인한 사용자 기준으로 동작한다.

**구현 범위**

- **회원가입·로그인·로그아웃** `/signup`, `/login`, 상단 오른쪽 "로그아웃": 닉네임(2~10자 한글·영문·숫자)과 비밀번호(8~64자)로 가입하면 같은 이름의 채널이 생기고 곧바로 로그인된다. 비밀번호는 Argon2id 해시로만 저장한다. 로그인하면 서버가 무작위 세션 토큰을 `streamapp_session` 쿠키(HttpOnly, SameSite=Lax, 30일)로 주고, DB `sessions`에는 토큰의 SHA-256만 남긴다. 요청마다 서버가 이 토큰으로 사용자를 찾는다(쿠키를 고쳐 다른 사람이 될 수 없다). 로그아웃하면 DB의 세션과 쿠키를 모두 지운다. 로그인 실패는 닉네임이 없든 비밀번호가 틀리든 "닉네임 또는 비밀번호가 맞지 않습니다"(401)다. 결정 근거는 [ADR 0005](docs/adr/0005-password-login-with-db-sessions.md).
  - 코드: 세션 동작 [service/auth.rs](packages/api/src/service/auth.rs), 쿠키 읽기·쓰기 [server/current_user.rs](packages/api/src/server/current_user.rs), 서버 함수 [users.rs](packages/api/src/users.rs), 화면 [views/auth.rs](packages/web/src/views/auth.rs), 마이그레이션 [0004](packages/api/migrations/0004_auth_and_comments.sql).
- **방송 소유자와 서버 권한 검사**: 채널 주인 = 방송 주인이다(사용자 1명 = 채널 1개). 화면에서 버튼을 숨기는 것과 별개로 서버가 모든 요청을 다시 확인한다. 로그인하지 않았으면 401, 주인이 아니면 403, 없는 대상이면 404다.

  | 동작 | 주소 | 비로그인 | 다른 사용자 | 주인 |
  | --- | --- | --- | --- | --- |
  | 녹화방송 등록 | `POST /api/recordings/upload` | 401 | (자기 채널에 등록) | 201 |
  | 녹화방송 제목 수정 | `POST /api/recordings/rename` | 401 | 403 | 200 |
  | 녹화방송 삭제(영상 파일·댓글 포함) | `POST /api/recordings/delete` | 401 | 403 | 200 |
  | 휴대폰 송출 시작 | `POST /api/broadcast/whip` | 401 | (자기 채널로 송출) | 201 |
  | 휴대폰 송출 종료 | `DELETE /api/broadcast/whip/<세션>` | 401 | 403 | 200 |
  | 내 채널 설정·송출 키 조회 | `GET /api/my-channel` | 401 | (자기 채널만) | 200 |
  | 다음 라이브 제목·진행 중인 라이브 제목 수정 | `POST /api/my-channel/next-live-title`, `/live-title` | 401 | (자기 채널만) | 200 |

  - 녹화방송 수정·삭제는 앱 서비스가 녹화방송 채널의 주인인지 확인한다([service/recordings.rs](packages/api/src/service/recordings.rs)의 `check_recording_owner`). 다른 사용자가 빈 제목을 보내도 제목 규칙보다 주인 확인이 먼저라 403이다.
  - 휴대폰 송출: 앱이 시작한 송출 세션마다 어느 채널의 것인지 기억해 두고, 종료 요청이 다른 채널 주인의 것이면 MediaMTX로 넘기지 않고 403으로 거절한다([whip.rs](packages/api/src/whip.rs)). 송출 시작은 로그인한 사용자 자기 채널로만 되므로 남의 채널로 송출할 방법이 없다. OBS 송출은 채널 주인만 볼 수 있는 송출 키로 MediaMTX가 앱에 확인한다(②와 같다).
  - 채널 설정·라이브 제목은 언제나 로그인한 사용자 자신의 채널만 다루는 주소라서 남의 채널에 닿을 수 없다.
- **댓글**: 녹화방송 상세 `/recordings/<번호>`와 라이브 시청 페이지 `/live/<채널ID>` 아래에 방송별 댓글이 있다. 라이브 댓글은 그 라이브 회차에 붙어 라이브가 끝나도 DB에 남는다(채팅은 ②처럼 저장하지 않는다). 내용은 1~500자.
  - 쓰기 `POST /api/comments`: 로그인한 사용자가 작성자로 저장된다. 비로그인은 서버가 401로 거절한다.
  - 수정 `POST /api/comments/edit`·삭제 `POST /api/comments/delete`: 작성자만 할 수 있다. 다른 사용자(녹화방송 주인 포함)는 403이다. 화면에는 자기 댓글에만 "수정"·"삭제"가 보인다.
  - 목록 `GET /api/recordings/<번호>/comments`, `GET /api/lives/<라이브ID>/comments`: 오래된 순, 작성자 닉네임·작성 시각·수정 여부.
  - 코드: [service/comments.rs](packages/api/src/service/comments.rs), 서버 함수 [comments.rs](packages/api/src/comments.rs), 화면 [views/comments.rs](packages/web/src/views/comments.rs).
- **공개 범위**: 로그인하지 않아도 할 수 있는 것은 홈의 라이브·녹화방송 목록, 라이브 시청(HLS), 녹화방송 재생(`/media/recordings/…`), 채팅 읽기, 댓글 읽기다. 방송 서비스라 시청은 누구에게나 열어 두었다. 로그인해야 하는 것은 녹화방송 등록·수정·삭제, 휴대폰 송출, 내 채널 설정(송출 키), 채팅 보내기, 댓글 쓰기·수정·삭제다.
- **이전 단계 사용자**: 비밀번호 없이 만든 사용자에게는 앱이 시작할 때 기본 비밀번호(`STREAMAPP_DEFAULT_PASSWORD`, 기본값 `test1234`)를 넣는다. 빈 DB에 미리 만드는 철수·영희·민수도 같은 비밀번호다. 로컬 확인용 시험 값이다.
- 이번 단계에서 하지 않은 것: 비밀번호 변경·재설정, 로그인 시도 횟수 제한, 회원 탈퇴, 관리자 권한, 방송 주인의 남의 댓글 지우기, 댓글 실시간 갱신(새로고침하면 보인다).

**직접 확인 절차(비로그인 / 방송 소유자 / 다른 사용자)**

준비: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Port 8090`로 띄우고 `http://localhost:8090`을 연다. 기본 사용자 철수·영희의 비밀번호는 `test1234`다. 서로 다른 사용자를 동시에 쓰려면 일반 창과 시크릿 창을 함께 쓰거나, 한 창에서 로그아웃·로그인을 바꿔 가며 한다.

1. **회원가입·로그인·로그아웃**: "회원가입"에서 새 닉네임과 비밀번호 7자를 넣으면 "비밀번호는 8~64자여야 합니다"로 거절된다. 8자 이상으로 가입하면 홈으로 가고 상단 오른쪽에 닉네임과 "로그아웃"이 보인다. "로그아웃" 후 "로그인"에서 틀린 비밀번호를 넣으면 "닉네임 또는 비밀번호가 맞지 않습니다"가 보이고, 맞게 넣으면 로그인된다. 개발자 도구 → Application → Cookies에서 `streamapp_session`이 HttpOnly인지 볼 수 있다.
2. **소유자로 녹화방송 등록**: 철수로 로그인해 "녹화방송 등록"에서 영상을 올린다(①의 3~5번). 상세 페이지에 "녹화방송 관리"(제목 저장, 녹화방송 삭제)가 보인다. 제목을 바꾸면 바로 반영된다.
3. **비로그인 확인**: 로그아웃한 창(또는 시크릿 창)에서 같은 상세 페이지를 연다. 영상과 댓글은 보이지만 관리 칸이 없고, 댓글 칸에는 "로그인이 필요합니다"가 보인다. "녹화방송 등록"·"휴대폰 방송"·"내 채널 설정"도 로그인 안내만 보인다. 버튼 숨기기가 아니라 서버가 거절하는지 보려면 그 창의 개발자 도구 콘솔에서 아래를 실행한다. 모두 `401 로그인이 필요합니다`가 나온다.

   ```js
   const post = async (url, body) => { const r = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) }); return r.status + " " + ((await r.json()).data?.ServerError?.message ?? ""); };
   await post("/api/comments", { target: { Recording: 1 }, content: "비로그인 댓글" });
   await post("/api/recordings/rename", { id: 1, title: "바꿈" });
   await post("/api/recordings/delete", { id: 1 });
   ```

4. **다른 사용자 확인**: 영희로 로그인해 철수의 녹화방송 상세를 연다. 관리 칸이 없다. 콘솔에서 3번 코드(`post` 정의와 `rename`·`delete` 줄)를 실행하면 `403 방송 주인만 할 수 있습니다`가 나오고, 새로고침해도 제목과 영상이 그대로다. 없는 번호(`id: 999`)는 `404 없는 녹화방송입니다`다.
5. **댓글**: 영희로 댓글을 쓰면 목록에 "영희 · 방금 전"으로 붙고, 자기 댓글에만 "수정"·"삭제"가 보인다. 고치면 "수정됨"이 붙는다. 철수(녹화방송 주인)로 바꿔 로그인하면 영희 댓글에 버튼이 없고, 콘솔에서 아래를 실행하면 둘 다 `403 댓글을 쓴 사람만 고치거나 지울 수 있습니다`다.

   ```js
   const id = (await (await fetch("/api/recordings/1/comments")).json()).comments[0].id;
   await post("/api/comments/edit", { id, content: "남의 댓글 고치기" });
   await post("/api/comments/delete", { id });
   ```

6. **송출 제어**: 철수로 로그인해 "휴대폰 방송"에서 방송을 시작·종료한다(②의 절차). 로그아웃한 창에서는 "휴대폰 방송"이 로그인 안내만 보이고, 송출 시작 요청은 서버가 401로 거절한다. 라이브 중에 시청 페이지 아래 댓글 칸에 로그인한 사용자가 댓글을 남길 수 있고, 라이브가 끝난 뒤에도 그 댓글은 DB에 남는다.
7. **삭제**: 철수로 녹화방송 상세의 "녹화방송 삭제" → "삭제"를 누르면 홈으로 가고, 목록에서 사라지며 그 주소는 "녹화방송을 찾을 수 없습니다"가 된다(영상 파일과 댓글도 지워진다).
8. **이전 단계 기능**: 홈 목록, 녹화방송 재생(건너뛰기), 라이브 시청·채팅이 로그인하지 않아도 그대로 된다.

자동 테스트: 회원가입·로그인·세션(만료·로그아웃 포함)은 [tests/auth.rs](packages/api/tests/auth.rs), 녹화방송 주인 권한은 [tests/recordings.rs](packages/api/tests/recordings.rs) 끝부분, 댓글과 작성자 권한은 [tests/comments.rs](packages/api/tests/comments.rs), 세션 쿠키 없는 올리기(401)는 [tests/recording_http.rs](packages/api/tests/recording_http.rs), 송출 시작·종료의 401·403·404는 [tests/whip_http.rs](packages/api/tests/whip_http.rs)가 확인한다.

### ④ 도네이션

③까지의 기능(녹화방송, 휴대폰 송출·시청, 회원가입·로그인, 방송 주인 권한 검사, 댓글)은 그대로이고, 방송마다 방송자(채널 주인)에게 후원하는 기능을 더했다. 결제는 **토스페이먼츠 테스트 모드**로 하며 실제 돈은 나가지 않는다. 결정 근거는 [ADR 0006](docs/adr/0006-donations-confirmed-by-server-with-toss-test-mode.md).

**구현 범위**

- **후원 대상·금액 선택**: 녹화방송 상세 `/recordings/<번호>`와 라이브 시청 페이지 `/live/<채널ID>`에 "후원" 칸이 있다. 받는 사람은 그 방송의 채널 주인으로 표시되고, 금액은 1,000·5,000·10,000·50,000원 버튼이나 직접 입력(1,000~1,000,000원), 메시지는 0~100자다. 로그인하지 않았으면 로그인 안내가, 자기 방송이면 "내 방송에는 후원할 수 없습니다"가 보인다.
- **테스트 결제 요청** `POST /api/donations/prepare`: 로그인한 사용자만(401). 서버가 방송에서 받는 채널을 정하고 금액·메시지를 확인해 무작위 주문번호로 `pending` 주문을 DB에 만든다. 화면은 돌려받은 주문 그대로 토스페이먼츠 결제창(JS SDK v2, `requestPayment`)을 연다. 금액을 화면에서 고쳐도 서버의 주문 금액은 바뀌지 않는다.
- **서버의 승인 확인 후 확정** `POST /api/donations/confirm`: 결제창이 성공 주소 `/donations/success?paymentKey=…&orderId=…&amount=…`로 돌아오면, 화면은 성공이라고 표시하지 않고 이 값을 서버에 넘긴다. 서버는
  1. 주문한 사람인지(아니면 403),
  2. 돌아온 금액이 주문 금액과 같은지(다르면 결제 승인을 요청하지 않고 주문을 `failed`로 닫는다),
  3. 시크릿 키로 토스페이먼츠 결제 승인 API(`POST /v1/payments/confirm`, 주문 금액, 주문번호를 멱등키로)를 부르고,
  4. 돌려받은 결제의 주문번호·금액·상태(`DONE`)가 주문과 같은지,
  5. 받는 채널이 지금도 그 방송의 채널이고 후원한 사람 자신이 아닌지

  를 확인한 뒤에만 `confirmed`로 바꾼다. 결과 페이지는 서버가 확인한 결과("철수님에게 5,000원 후원을 확정했습니다")만 보여 준다.
  - 코드: 확정 경로 [service/donations.rs](packages/api/src/service/donations.rs)의 `confirm_donation`·`settle`, 결제 승인·조회 클라이언트 [server/toss_payments.rs](packages/api/src/server/toss_payments.rs), 서버 함수 [donations.rs](packages/api/src/donations.rs), 화면 [views/donations.rs](packages/web/src/views/donations.rs)(클라이언트 키가 `test_gck_`면 주문서형 결제 UI, `test_ck_`면 결제창), 마이그레이션 [0005](packages/api/migrations/0005_donations.sql).
- **승인 통지 중복 방지**: 같은 승인 통지가 여러 번 와도 한 번만 적립한다.
  - 저장 구조: `donations.order_id`와 `donations.payment_key`가 UNIQUE라 한 결제가 두 후원이 될 수 없다. 상태는 `pending → confirmed / canceled / failed`이고, `confirmed`일 때만 `payment_key`·`confirmed_at_ms`가 있다(CHECK 제약).
  - 확정은 `UPDATE … SET status = 'confirmed' … WHERE order_id = ? AND status <> 'confirmed'` 한 번이다. 두 번째 통지는 바뀌는 행이 없어 "이미 확정된 후원입니다"로 돌아가고 합계는 그대로다.
  - 이미 `confirmed`인 주문에 같은 승인 값이 다시 오면(성공 페이지 새로고침) 결제 서비스를 다시 부르지 않는다. 동시에 두 요청이 와도 하나만 확정되고, 결제 서비스가 `ALREADY_PROCESSED_PAYMENT`를 주면 결제를 조회해 같은 결제인지 확인한다.
  - **웹훅** `POST /api/payments/toss/webhook`(`PAYMENT_STATUS_CHANGED`): 본문은 믿지 않고 결제 키로 결제를 조회해 같은 확정 경로를 탄다. 성공 주소와 웹훅이 모두 와도, 웹훅이 재전송되어도 한 번만 적립한다. 조회 결과가 만료·취소면 기다리던 주문을 `failed`로 닫는다. 결제 서비스에 연결하지 못하면 500을 돌려 결제 서비스가 다시 보내게 한다. 코드: [payment_webhook.rs](packages/api/src/payment_webhook.rs).
- **취소·실패**: PC에서 결제창을 닫으면(`USER_CANCEL`) 화면이 `POST /api/donations/abandon`으로 주문을 `canceled`로 닫는다. 실패 주소 `/donations/fail?code=…&message=…&donation=<주문번호>`로 돌아오면 `PAY_PROCESS_CANCELED`는 `canceled`, 그 밖은 `failed`로 닫고 사유를 보여 준다. 화면의 실패 표시로 이미 `confirmed`인 후원을 되돌리지는 않는다.
- **방송별 후원 내역과 합계** `GET /api/recordings/<번호>/donations`, `GET /api/lives/<라이브ID>/donations`: 그 방송에서 확정된 후원만 최근 확정순(후원자 닉네임, 금액, 메시지, 확정 후 지난 시간)으로 보이고, 제목에 "후원 N건 · 합계 X원"이 보인다. 결제를 기다리는 주문과 취소·실패한 주문은 들어가지 않는다. 라이브 후원은 그 라이브 회차에 붙어 같은 채널의 녹화방송 합계와 섞이지 않는다. 로그인하지 않아도 볼 수 있다.
- **결제 설정 없음**: 테스트 키 환경변수가 없거나 `test_`로 시작하지 않으면 결제를 켜지 않는다. 이때 후원 내역은 보이고 후원 칸에는 설정 안내가 보이며, 주문 요청은 503이다.
- **삭제와 기록**: 녹화방송을 지워도 후원 기록은 남는다(`recording_id`만 비고 받는 채널은 그대로).
- 이번 단계에서 하지 않은 것: 결제 취소(환불)·부분 취소, 금액이 어긋난 승인 결제의 자동 취소(로그와 `failed` 기록만 남김), 가상계좌, 채팅·화면 후원 알림, 정산, 채널 전체 후원 합계, 내 후원 내역 페이지.

**환경변수와 테스트 키 준비**

1. 테스트 키 한 쌍을 준비한다. 두 종류 모두 된다.
   - **주문서형(결제위젯) 키** `test_gck_…`·`test_gsk_…`: 토스페이먼츠 개발자센터의 주문서형 연동 가이드에 나오는 **문서용 테스트 키**를 가입 없이 그대로 쓸 수 있다. 후원 칸 아래에 토스 결제수단·약관 UI와 "결제하기"·"취소" 버튼이 그려진다.
   - **API 개별 연동 키** `test_ck_…`·`test_sk_…`: [개발자센터 API 키 메뉴](https://developers.tosspayments.com/my/api-keys)에서 가입 후 확인한다(전자결제 계약 없이 된다). "후원하기"를 누르면 카드 결제창이 바로 열린다.

   클라이언트 키와 시크릿 키는 같은 종류의 짝이어야 한다.
2. 앱을 띄우는 PowerShell 창에서 환경변수를 정하고 `dev.ps1`을 실행한다. 시크릿 키는 저장소나 README에 적지 않는다.

   ```powershell
   $env:STREAMAPP_TOSS_CLIENT_KEY = "test_gck_..."   # 또는 test_ck_...
   $env:STREAMAPP_TOSS_SECRET_KEY = "test_gsk_..."   # 또는 test_sk_...
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Port 8090
   ```

   로그에 `후원 결제: 토스페이먼츠 테스트 모드`가 찍히면 준비된 것이다. 키가 없으면 `후원 결제를 끕니다` 경고가 찍힌다.
3. (선택) 웹훅: 개발자센터 → 웹훅에서 `PAYMENT_STATUS_CHANGED`로 `https://<공개 주소>/api/payments/toss/webhook`을 등록한다. 로컬 PC에는 공개 주소가 없으므로 ②처럼 `cloudflared tunnel --url http://127.0.0.1:8090`로 받은 `https://….trycloudflare.com`을 쓴다. 웹훅 없이도 성공 주소로 확정된다.

**직접 확인 절차(테스트 결제 성공·실패·취소, 승인 통지 중복)**

준비: 위 환경변수로 앱을 띄우고 `http://localhost:8090`을 연다. 철수(`test1234`)로 로그인해 녹화방송을 하나 올리고(①의 절차), 로그아웃한 뒤 영희(`test1234`)로 로그인해 그 녹화방송 상세를 연다. 후원 칸에 "받는 사람 철수"와 "후원 0건 · 합계 0원"이 보인다.

1. **테스트 결제 성공**: 5,000원을 고르고 메시지를 적어 "5,000원 후원하기"를 누른다. 주문서형 키면 그려진 결제 UI에서 결제수단(예: 신용·체크카드 → 카드사)을 고르고 "5,000원 결제하기"를 누른다. 토스페이먼츠 테스트 결제창에서 카드사 인증까지 진행한다(테스트 환경이라 실제로 결제되지 않는다. 카드사에 따라 앱·간편 로그인을 요구하므로 본인이 쓸 수 있는 방식을 고른다). `/donations/success?...`로 돌아와 "서버가 결제 승인을 확인하는 중입니다…" 다음에 "철수님에게 5,000원 후원을 확정했습니다"가 보인다. "방송으로 돌아가기"를 누르면 "후원 1건 · 합계 5,000원"과 "영희 5,000원 · 방금 전"이 보인다. 개발자센터 → 결제내역에도 테스트 결제가 보인다.
2. **승인 통지 중복(성공 주소 다시 받기)**: 1번의 결과 페이지에서 새로고침(F5)한다. 같은 `paymentKey`·`orderId`·`amount`가 서버에 다시 가지만 "이미 확정된 후원입니다. 같은 결제를 다시 받아도 한 번만 적립합니다."가 보이고, 방송으로 돌아가면 여전히 "후원 1건 · 합계 5,000원"이다.
3. **승인 통지 중복(웹훅 재전송)**: 웹훅을 등록했다면 개발자센터 → 웹훅 → 전송 기록에서 1번 결제의 기록을 골라 "다시 시도"를 두세 번 누른다. 앱 로그에 `이미 확정된 결제의 통지를 다시 받았습니다(다시 적립하지 않음)`이 찍히고 합계는 그대로다. 웹훅을 등록하지 않았다면 직접 보낸다(1번 결제의 `paymentKey`는 결과 페이지 주소창에 있다). 본문의 금액·주문번호는 무시하고 결제 서비스 조회 결과로 처리하므로, 금액을 바꿔 보내도 합계가 바뀌지 않는다.

   ```powershell
   $body = '{"eventType":"PAYMENT_STATUS_CHANGED","data":{"paymentKey":"<1번의 paymentKey>","orderId":"x","totalAmount":999999,"status":"DONE"}}'
   1..3 | ForEach-Object { (Invoke-WebRequest -Method Post -Uri http://localhost:8090/api/payments/toss/webhook -ContentType "application/json" -Body $body).StatusCode }
   ```

   세 번 모두 `200`이고 방송의 합계는 그대로 5,000원이다.
4. **테스트 결제 취소**: 다시 "후원하기" → 결제창에서 닫기(X)를 누른다. 주문서형 키면 결제 UI가 남아 다시 고를 수 있고, 결제 UI의 "취소"를 누르면 후원 칸에 "결제를 취소했습니다. 후원하지 않았습니다."가 보이며 주문이 `canceled`로 닫힌다. 결제창 방식(API 개별 연동 키)은 결제창을 닫으면 바로 같은 문구가 보인다. 휴대폰(페이지 이동 방식)에서는 `/donations/fail` 페이지에 "후원 결제 취소"가 보인다. 합계는 그대로다.
5. **테스트 결제 실패**:
   - 결제창의 실패: 결제창 진행 중 실패하면 결제창이 `/donations/fail?code=…&message=…`로 보내고 "후원 결제 실패"와 사유가 보이며 주문은 `failed`로 닫힌다.
   - 승인 만료: 결제창에서 인증을 마치고 돌아온 성공 주소를 복사해 두었다가, 10분 넘게 지난 뒤 연다(결제 서비스는 인증 후 10분 안에 승인하지 않은 결제를 만료시킨다). "후원을 확정하지 못했습니다: 결제가 승인되지 않았습니다: …"가 보이고 합계는 그대로다.
   - 금액 조작과 실패 주소: 영희로 로그인한 창의 개발자 도구 콘솔에서 아래를 실행한다. 결제창을 열지 않고 주문만 만든 뒤 (1) 금액을 100원으로 고쳐 승인을 요청하면 서버가 결제 서비스에 묻지도 않고 `400 결제 금액이 주문과 다릅니다. 후원을 확정하지 않았습니다`로 거절하고, (2) 다른 주문으로 실패 주소를 열면 "결제에 실패했습니다: 테스트실패 (PAY_PROCESS_ABORTED)"가 보인다. 두 경우 모두 합계는 그대로다.

     ```js
     const post = async (url, body) => { const r = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) }); return r.status + " " + ((await r.json()).data?.ServerError?.message ?? ""); };
     const order = async () => (await fetch("/api/donations/prepare", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ target: { Recording: 1 }, amount: 5000, message: "" }) })).json();
     const a = await order();
     await post("/api/donations/confirm", { approval: { payment_key: "tampered", order_id: a.order_id, amount: 100 } });
     const b = await order();
     location.href = `/donations/fail?code=PAY_PROCESS_ABORTED&message=${encodeURIComponent("테스트실패")}&donation=${b.order_id}`;
     ```

6. **서버 권한 검사**: 로그아웃한 창의 콘솔에서 아래 첫 줄을 실행하면 `401 로그인이 필요합니다`다. 로그인한 창에서 둘째 줄은 `400 후원 금액은 1,000원 이상 1,000,000원 이하여야 합니다`다. 철수(녹화방송 주인)는 후원 칸 대신 "내 방송에는 후원할 수 없습니다"가 보이고, 같은 요청을 보내면 `400 자기 방송에는 후원할 수 없습니다`다. 영희가 만든 주문번호로 민수가 `/api/donations/confirm`을 부르면 `403 후원한 사람만 처리할 수 있습니다`다.

   ```js
   const post = async (url, body) => { const r = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) }); return r.status + " " + ((await r.json()).data?.ServerError?.message ?? ""); };
   await post("/api/donations/prepare", { target: { Recording: 1 }, amount: 5000, message: "" });
   await post("/api/donations/prepare", { target: { Recording: 1 }, amount: 500, message: "" });
   ```

7. **라이브 후원**: 철수로 휴대폰 방송(②)이나 테스트 송출을 시작하고, 영희로 시청 페이지 `/live/<채널ID>`의 후원 칸에서 1번처럼 후원한다. 그 라이브의 합계에만 더해지고 녹화방송 합계와 섞이지 않는다.
8. **이전 단계 기능**: 녹화방송 등록·재생, 휴대폰 송출·시청, 회원가입·로그인·로그아웃, 댓글 쓰기·수정·삭제와 방송 주인 권한 검사(③의 절차)가 그대로 된다.

자동 테스트: [tests/donations.rs](packages/api/tests/donations.rs)(21개)가 가짜 결제 서비스로 주문 규칙(금액 범위, 메시지 길이, 자기 방송, 없는 방송, 결제 설정 없음), 확정과 방송별 내역·합계, 같은 승인 두 번·동시 승인·웹훅 반복·웹훅 뒤 성공 주소에서 한 번만 적립, 금액 조작·결제 서비스와 금액 불일치·결제 거절·연결 실패·남의 주문, 취소·실패 기록, 만료 통지, 라이브 후원, 녹화방송 삭제 뒤 기록 유지를 확인한다. [tests/payments_http.rs](packages/api/tests/payments_http.rs)(7개)는 가짜 토스페이먼츠 서버로 승인 요청의 인증 헤더·멱등키·본문, `ALREADY_PROCESSED_PAYMENT` 뒤 조회, 거절·처리 중·서버 오류 구분, 웹훅 반복 통지가 한 번만 적립되는지를 확인한다. 실제 토스페이먼츠에는 접속하지 않는다.

### ⑤ 로컬 오토스케일

①~④의 기능(녹화방송, 휴대폰 송출·시청, 회원가입·로그인과 권한 검사, 댓글, 후원)을 그대로 두고, 앱을 컨테이너 이미지로 만들어 **로컬 쿠버네티스(kind)** 에서 여러 인스턴스로 실행한다. CPU 사용률에 따라 HPA가 앱 파드를 2~5개로 늘리고 줄이며, 상태(DB·영상 파일·로그인 세션·휴대폰 송출 세션·라이브·댓글·채팅)는 인스턴스 밖 공용 저장소에 둔다. 결정 근거는 [ADR 0007](docs/adr/0007-multi-instance-state-shared-sqlite-and-redis-chat.md), 실제 실행 기록은 [docs/autoscale-verification.md](docs/autoscale-verification.md).

**구현 범위**

| 과제 요구 | 구현 |
| --- | --- |
| 서버 컨테이너 이미지 | [Dockerfile](Dockerfile)(멀티 스테이지: `rust:1.99.1-trixie` + dx 0.7.10 체크섬 확인 → `dx bundle --web --release --package web` → `debian:trixie-slim`, 비루트 UID 10001, `server` + `public/`), [.dockerignore](.dockerignore). 이미지 이름 `streamapp:local` |
| 로컬 클러스터에서 여러 인스턴스 | kind 클러스터 [k8s/kind-cluster.yaml](k8s/kind-cluster.yaml)(노드 1개), 앱 Deployment `streamapp`(replicas 2, readiness·liveness·startup probe `GET /healthz`) [k8s/app/streamapp.yaml](k8s/app/streamapp.yaml) |
| 지표 기준 최소·최대 자동 조절 | HPA `autoscaling/v2` [k8s/app/hpa.yaml](k8s/app/hpa.yaml): CPU 평균 50%(requests 100m 기준), 최소 2·최대 5, 늘리기는 기다리지 않고 15초마다 최대 2개, 줄이기는 60초 안정화 뒤 15초마다 1개. 지표는 metrics-server v0.9.1 [k8s/metrics-server/](k8s/metrics-server/)(kind용 `--kubelet-insecure-tls` 덧붙임) |
| 여러 인스턴스 앞 요청 분산 | Service `streamapp`(NodePort 30080 → 이 PC의 18080). kube-proxy가 준비된 파드 사이에 연결을 나눈다. 세션이 DB에 있어 고정 세션이 필요 없다. 응답 헤더 `x-streamapp-instance`에 답한 파드 이름이 실린다(`STREAMAPP_INSTANCE_NAME`, [instance.rs](packages/api/src/instance.rs)) |
| 상태를 인스턴스 밖 공용 저장소에 | 아래 "공용 저장소 설계와 한계" |
| 부하 발생 | k6 스크립트 [loadtest/autoscale.js](loadtest/autoscale.js)(홈 SSR `/`·`/api/recordings`·`/api/live-channels`·`/healthz`를 4:2:2:1로, 요청마다 새 연결), 실행·관찰 [scripts/load-test.ps1](scripts/load-test.ps1), [scripts/k8s-watch.ps1](scripts/k8s-watch.ps1) |

그 밖에 MediaMTX 1대([k8s/app/mediamtx.yaml](k8s/app/mediamtx.yaml), 설정 [k8s/app/mediamtx.yml](k8s/app/mediamtx.yml): 인증·송출 훅이 앱 Service `http://streamapp:8080`으로 간다. 공식 이미지 `1.21.3-ffmpeg`에는 curl이 없어 훅은 busybox `wget`으로 POST한다), Redis 1대([k8s/app/redis.yaml](k8s/app/redis.yaml)), 공유 볼륨([k8s/app/storage.yaml](k8s/app/storage.yaml))을 같은 네임스페이스 `streamapp`에 두고, [k8s/app/kustomization.yaml](k8s/app/kustomization.yaml)로 한 번에 적용한다.

```text
 이 PC(브라우저·OBS·k6)
   │ :18080              │ :1935 RTMP, :8888 HLS, :8189 ICE
 ┌─┴──────────────── kind 노드(1개) ───────────────────────────────┐
 │ Service streamapp(NodePort)       Service mediamtx-public(NodePort)│
 │   │ 요청 분산                          │                          │
 │ streamapp 파드 ×2~5(HPA) ◀─ 인증·훅 ─ MediaMTX                    │
 │   │ │                                                            │
 │   │ └─ pub/sub ─▶ Redis(채팅 중계)                                │
 │   └─── /data ──▶ PVC streamapp-data(SQLite DB + 녹화방송 영상)    │
 │ metrics-server ─▶ HPA(CPU 50%, 최소 2·최대 5)                     │
 └──────────────────────────────────────────────────────────────────┘
```

**공용 저장소 설계와 한계**

| 상태 | 어디에 | 코드 |
| --- | --- | --- |
| 사용자·채널·라이브·녹화방송·댓글·후원, 로그인 세션 | 공유 볼륨 `/data/streamapp.db`(SQLite, WAL·busy_timeout 5초) | 이미 DB에 있다. 동시에 시작한 파드끼리 연결·마이그레이션이 겹치면 다시 시도한다([db.rs](packages/api/src/service/db.rs) `connect_file`). 기본 사용자는 `BEGIN IMMEDIATE`로 한 번만 만든다([users.rs](packages/api/src/service/users.rs)) |
| 녹화방송 영상 파일 | 공유 볼륨 `/data/recordings/` | 기존 `STREAMAPP_RECORDINGS_DIR` |
| 휴대폰 송출 세션 → 채널(송출 종료 권한 확인) | DB 표 `whip_sessions`(마이그레이션 [0006](packages/api/migrations/0006_whip_sessions.sql)) | 이전에는 인스턴스 메모리. [whip.rs](packages/api/src/whip.rs), [whip_sessions.rs](packages/api/src/service/whip_sessions.rs). 종료 요청이 다른 파드로 가도 403·404·200이 같다 |
| 채팅 이벤트(메시지·제목 변경·라이브 종료) | 인스턴스 메모리 + Redis pub/sub(`streamapp:chat:<채널ID>`) | [chat_relay.rs](packages/api/src/service/chat_relay.rs). `STREAMAPP_REDIS_URL`이 없으면 메모리만 쓴다(⑥ 그대로). Redis에 못 붙어도 앱은 뜨고 그 인스턴스 안 채팅은 되며, 다시 연결한다 |
| 라이브 상태 | DB(MediaMTX 훅이 어느 파드로 가도 반영) | `STREAMAPP_MULTI_INSTANCE=true`면 시작 때 MediaMTX에 닿지 못해도 열린 라이브를 닫지 않는다([publish.rs](packages/api/src/service/publish.rs) `keeping_lives_if_unreachable`) |

- **전제**: 노드가 하나라 모든 앱 파드가 같은 노드에서 ReadWriteOnce local-path 볼륨(노드의 폴더 하나)을 함께 마운트한다. 같은 커널·파일 시스템이라 SQLite의 파일 잠금과 WAL 공유 메모리가 파드 사이에서도 동작한다.
- **한계**: 노드가 여럿이면 다른 노드의 파드는 이 볼륨을 마운트하지 못하고, 네트워크 파일 시스템 위의 SQLite는 잠금을 믿을 수 없다. 그때는 DB를 Postgres로, 영상을 오브젝트 스토리지(S3 호환)로 옮겨야 한다. SQLite는 쓰기를 한 번에 하나씩 하므로 파드를 늘려도 쓰기 처리량은 늘지 않는다. MediaMTX·Redis는 1대다.
- **⑥ 배포(단일 VM)와의 차이**: VM은 `STREAMAPP_REDIS_URL`·`STREAMAPP_MULTI_INSTANCE`를 정하지 않으므로 채팅은 메모리 그대로이고, 시작 때 MediaMTX에 닿지 못하면 열린 라이브를 닫는(꺼져 있던 동안 놓친 종료 알림 보정) 이전 동작 그대로다. 새 마이그레이션 0006(`whip_sessions`)은 다음 배포 때 앱이 시작하면서 자동으로 적용된다.

**실행 방법**

필요한 도구: Docker Desktop(엔진 실행 중, 메모리 6GB 이상 권장), `kubectl`(Docker Desktop에 들어 있다). kind v0.33.0은 스크립트가 `.tools\`로 내려받고(체크섬 확인), k6는 Docker 이미지 `grafana/k6:1.8.3`로 돌린다. Rust·dx는 이미지 안에서 빌드하므로 PC에 없어도 된다. 로컬 개발(`dev.ps1`)의 MediaMTX와 같은 포트(1935·8888·8189)를 쓰므로 둘을 함께 켜지 않는다.

```powershell
# 1. 클러스터 만들기 → metrics-server → 이미지 빌드(처음 10~20분)·kind에 넣기 → 매니페스트 적용 → 준비 대기
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/k8s-up.ps1          # 이미지를 다시 빌드하지 않으려면 -SkipBuild
# 2. 브라우저에서 http://localhost:18080 (빈 DB면 철수·영희·민수, 비밀번호는 k8s\app\secret.env)
# 3. 부하 주기 + 파드 수 변화 기록(약 9분, loadtest\out\autoscale-<시각>.log, k6-<시각>.txt)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/load-test.ps1       # -Vus 60 -HoldSeconds 300 처럼 바꿀 수 있다
#    부하 없이 상태만 보기: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/k8s-watch.ps1
# 4. 지우기(클러스터와 그 안의 DB·영상이 사라진다)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/k8s-down.ps1
```

손으로 하려면: `docker build -t streamapp:local .` → `.tools\kind.exe create cluster --config k8s\kind-cluster.yaml` → `kubectl apply -k k8s\metrics-server` → `.tools\kind.exe load docker-image streamapp:local --name streamapp` → `copy k8s\app\secret.env.example k8s\app\secret.env` → `kubectl apply -k k8s\app`.

**환경변수 예시**(앱 ConfigMap [k8s/app/streamapp.yaml](k8s/app/streamapp.yaml), 비밀 값은 Secret [k8s/app/secret.env.example](k8s/app/secret.env.example) → `secret.env`(git 제외))

| 환경변수 | 클러스터 값 | 뜻 |
| --- | --- | --- |
| `IP`, `PORT` | `0.0.0.0`, `8080` | 컨테이너 안에서 듣는 주소 |
| `STREAMAPP_DB` | `/data/streamapp.db` | 공유 볼륨의 SQLite |
| `STREAMAPP_RECORDINGS_DIR` | `/data/recordings` | 공유 볼륨의 녹화방송 영상 폴더 |
| `STREAMAPP_REDIS_URL` | `redis://redis:6379` | **새 값.** 채팅을 다른 인스턴스로 전달할 Redis. 없으면 메모리만 쓴다 |
| `STREAMAPP_MULTI_INSTANCE` | `true` | **새 값.** 시작 때 MediaMTX에 닿지 못해도 라이브를 닫지 않는다 |
| `STREAMAPP_INSTANCE_NAME` | 파드 이름(Downward API) | **새 값.** 응답 헤더 `x-streamapp-instance`. 없으면 붙이지 않는다 |
| `STREAMAPP_MEDIAMTX_API`, `STREAMAPP_MEDIAMTX_WEBRTC` | `http://mediamtx:9997`, `http://mediamtx:8889` | 클러스터 안 MediaMTX |
| `STREAMAPP_HLS_BASE_URL`, `STREAMAPP_OBS_SERVER` | `http://localhost:8888`, `rtmp://localhost/` | 이 PC에서 닿는 공개 주소(kind 호스트 포트) |
| `STREAMAPP_SECURE_COOKIE` | `false` | http로 접속하므로 끈다 |
| `STREAMAPP_DEFAULT_PASSWORD` | `test1234`(Secret, 시험 값) | 빈 DB의 기본 사용자 비밀번호 |
| `STREAMAPP_TOSS_CLIENT_KEY`, `STREAMAPP_TOSS_SECRET_KEY` | 비어 있음(Secret) | 넣으면 후원 결제가 켜진다(④) |

**스케일 아웃/인 확인 절차**

1. `k8s-up.ps1`이 끝나면 `kubectl --context kind-streamapp -n streamapp get pods,hpa`에 앱 파드 2개가 `1/1 Running`, HPA가 `cpu: 1%/50%  2  5  2`로 보인다. `curl.exe -i http://localhost:18080/healthz`를 여러 번 보내면 `x-streamapp-instance`에 두 파드 이름이 번갈아 나온다.
2. 브라우저에서 회원가입·로그인하고 녹화방송을 올리고 댓글을 단다. (선택) `.\scripts\test-publish.ps1 -ChannelId <채널ID> -StreamKey <송출 키>`로 라이브를 켠다.
3. `load-test.ps1`을 실행한다. 기록에서 HPA CPU가 50%를 넘고 HPA 파드 수가 2 → 3 → 5로 늘며, 요청 분산 열에 새 파드 이름이 나타나는지 본다.
4. 부하 중에 브라우저를 새로 고쳐 로그인 상태·녹화방송·댓글·라이브가 그대로인지 본다(어느 파드가 답해도 같다).
5. 부하가 끝나면 CPU가 떨어지고 60초 뒤부터 15초마다 1개씩 2개까지 줄어드는지 본다. 줄어든 뒤에 4를 다시 확인한다.
6. `k8s-down.ps1`로 지운다.

**실제 확인 결과(2026-11-21)** — 전체 기록과 기능 확인 표는 [docs/autoscale-verification.md](docs/autoscale-verification.md)

`load-test.ps1` 기본값(가상 사용자 40, 유지 4분)에서 `k8s-watch.ps1`이 남긴 인스턴스 수 변화(발췌. 열: 시각 | HPA CPU(현재/목표) | HPA 파드 | 앱 파드 Ready/전체 | 파드별 CPU | `/healthz` 12번에 답한 파드):

```text
22:24:04 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=3 nw5wf=9
22:24:15 | 70%/50%           | 2        | 3/3                 | d5wsp=79m nw5wf=61m | 9vvmt=2 d5wsp=7 nw5wf=3
22:24:47 | 318%/50%          | 3        | 5/5                 | 9vvmt=343m d5wsp=302m nw5wf=311m | 4944w=2 9vvmt=3 d5wsp=3 nw5wf=2 wgsmv=2
22:25:30 | 222%/50%          | 5        | 5/5                 | 4944w=216m 9vvmt=237m d5wsp=202m nw5wf=220m wgsmv=235m | 4944w=4 9vvmt=2 d5wsp=3 nw5wf=1 wgsmv=2
22:28:43 | 194%/50%          | 5        | 5/5                 | 4944w=204m 9vvmt=170m d5wsp=181m nw5wf=236m wgsmv=183m | 4944w=3 9vvmt=1 d5wsp=5 nw5wf=1 wgsmv=2
22:29:15 | 1%/50%            | 5        | 5/5                 | 4944w=1m 9vvmt=1m d5wsp=4m nw5wf=1m wgsmv=1m | 4944w=4 9vvmt=2 nw5wf=3 wgsmv=3
22:29:46 | 1%/50%            | 5        | 4/4                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m wgsmv=1m | 4944w=1 9vvmt=3 d5wsp=5 nw5wf=3
22:30:07 | 1%/50%            | 4        | 3/3                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m | 9vvmt=3 d5wsp=4 nw5wf=5
22:30:30 | 1%/50%            | 2        | 2/2                 | 9vvmt=1m d5wsp=1m nw5wf=2m | d5wsp=6 nw5wf=6
```

- 스케일 아웃 2 → 3 → 5(부하 시작 뒤 약 45초), 부하가 끝나고 약 1분 뒤부터 스케일 인 5 → 4 → 3 → 2(약 45초).
- k6: 188,229 요청, 실패 0.00%(스케일 아웃·인 동안 포함), p95 4.75ms.
- 같은 세션 쿠키로 `GET /api/current-user` 12번: 파드 2개일 때, 5개일 때(5개 파드 모두가 답함), 다시 2개로 줄어든 뒤 모두 12/12 로그인 상태. 올린 녹화방송(영상 Range 206)·댓글·켜 둔 라이브가 처음부터 끝까지 그대로였다.
- 서로 다른 파드에 붙은 채팅 WebSocket 두 개 사이에 메시지가 전달되고(Redis), 송출을 끄자 다른 파드의 시청자에게 라이브 종료가 갔다. 두 파드가 빈 DB로 동시에 뜰 때 실제로 마이그레이션이 한 번 겹쳤고 다시 시도해 정상 시작했다.

자동 테스트: [tests/multi_instance.rs](packages/api/tests/multi_instance.rs)(같은 파일 DB에 따로 연결한 두 인스턴스로 동시 마이그레이션·기본 사용자 한 번, 세션 공유, 다른 인스턴스가 받은 훅 반영, MediaMTX에 닿지 못한 새 인스턴스의 라이브 유지, Redis가 꺼져도 인스턴스 안 채팅, 응답 헤더를 확인한다. Redis 중계 2개는 `#[ignore]`로 Redis가 있을 때만 돈다), [tests/whip_http.rs](packages/api/tests/whip_http.rs)의 다른 인스턴스로 간 송출 종료 요청.

- 이번 단계에서 하지 않은 것: 여러 노드 클러스터(Postgres·오브젝트 스토리지가 필요), MediaMTX·Redis 여러 대, 클러스터 안 HTTPS·인그레스, 클러스터에서의 휴대폰 송출(ICE 8189를 이 PC에만 열고 `webrtcAdditionalHosts`를 비워 두었다. 확인하지 않음), 이미지 레지스트리에 올리기, CPU 말고 다른 지표(메모리·요청 수)의 오토스케일, Redis가 끊긴 동안 놓친 채팅 다시 보내기.

### ⑥ 도메인·OCI 배포와 Windows/macOS 클라이언트

①~④의 기능(녹화방송, 휴대폰 송출·시청, 회원가입·로그인과 권한 검사, 댓글, 후원)을 그대로 두고, 백엔드를 OCI Always Free VM에 배포해 구매한 도메인의 HTTPS로 제공한다. 같은 Rust 코드베이스로 Windows·macOS 데스크톱 클라이언트를 만들어 다운로드 페이지에서 배포한다. 자세한 재현 절차는 [docs/deploy.md](docs/deploy.md)(서버)와 [docs/desktop-client.md](docs/desktop-client.md)(클라이언트)에 있다.

**공개 주소**

| 무엇 | 주소 |
| --- | --- |
| 서비스(웹) | https://smj0617.com |
| 상태 확인 | https://smj0617.com/healthz → `{"status":"ok","database":"ok","mediamtx":"ok","version":"0.7.0"}` |
| 다운로드 페이지 | https://smj0617.com/download |
| 설치 파일(GitHub Releases) | https://github.com/smj0617/StreamApp/releases/latest |

**도메인과 비용**

- 도메인: `smj0617.com`. **Cloudflare Registrar에서 등록함**(2026-11-21 등록, 만료 2027-11-21, 자동 갱신 켬).
- 최초 등록 비용: **US$11.17**(1년, 청구서 IN-90087080). 갱신 비용: **US$11.17/년**(Cloudflare 대시보드의 갱신 가격. Cloudflare는 등록 기관 원가로 팔아 등록가와 갱신가가 같다).
- 서버(OCI Always Free)와 TLS 인증서(Let's Encrypt)는 무료다. 이 과제에 든 돈은 도메인 비용뿐이다.

**DNS와 HTTPS 구성**

- DNS: Cloudflare DNS에 `A smj0617.com → 129.225.178.57`(VM 공인 IP), TTL 자동, **프록시 끔(DNS 전용)**. Cloudflare 프록시를 켜면 HTTP(80·443)만 지나가고 휴대폰 송출(WebRTC ICE 8189 UDP·TCP)과 OBS(RTMP 1935)가 막히기 때문이다.
- TLS: VM의 Caddy가 사이트 주소(`{$STREAMAPP_DOMAIN}` = `smj0617.com`)를 보고 **Let's Encrypt 인증서를 자동으로 받고 갱신**하며 http(80)를 https(443)로 넘긴다(308). 발급 조건은 A 레코드가 이 VM을 가리키고 80·443이 열려 있는 것.
- 경로 나누기([deploy/Caddyfile](deploy/Caddyfile)): `/internal/*` → 403(MediaMTX 인증·훅 엔드포인트는 인증이 없어 바깥에서 막는다), `/hls/*` → MediaMTX HLS(127.0.0.1:8888, MediaMTX가 주는 첫 리디렉트 주소에 `/hls`를 다시 붙인다), 그 밖 → 앱(127.0.0.1:8080, 채팅 WebSocket·녹화방송 업로드 510MB 포함).

**OCI VM**

| 항목 | 값 |
| --- | --- |
| 리전 | Japan Central (Osaka) `ap-osaka-1`, AD-1(홈 리전. Always Free 컴퓨트는 홈 리전에서만 만들 수 있다) |
| Shape | **VM.Standard.E2.1.Micro**(AMD EPYC, x86_64) — Always Free 대상 |
| OCPU / 메모리 | 1 OCPU(1/8 OCPU 기준 버스트) / **1 GB**(+ 스왑 파일 2 GB) |
| 디스크 | 부트 볼륨 약 47 GB(Always Free 블록 스토리지 200 GB 안) |
| OS | Canonical Ubuntu 24.04 Minimal(x86_64) |
| 공인 IP | 129.225.178.57(임시 공인 IP, 인스턴스를 지우지 않으면 유지) |
| 보안 목록 Ingress | TCP 22(SSH)·80·443·1935(RTMP)·8189(ICE), UDP 8189(ICE). VM 안 iptables에도 같은 허용 규칙을 넣었다 |

- **선택 근거**: Oracle 공식 문서 기준 우선 후보인 **ARM64 `VM.Standard.A1.Flex` 2 OCPU·12 GB를 먼저 시도했다.** 오사카 리전은 가용성 도메인이 하나(AD-1)뿐인데, 2026-11-21에 `2 OCPU/12 GB`와 `1 OCPU/6 GB` 모두 "AD-1 셰이프 용량이 부족합니다(Out of capacity)"로 만들어지지 않았다. 홈 리전은 바꿀 수 없고 과제 기한이 다음 날이라, Always Free에서 A1 다음으로 고를 수 있는 유일한 컴퓨트인 **E2.1.Micro**(Always Free로 2대까지)를 골랐다. 이 서비스는 영상을 다시 인코딩하지 않고(MediaMTX가 RTMP·WebRTC를 HLS로 다시 묶기만 함), 앱은 Rust 단일 실행 파일이라 1 GB에서도 돈다. 서버에서는 빌드하지 않고 GitHub Actions가 만든 linux-amd64 묶음만 올린다. 메모리 여유를 위해 스왑 2 GB를 켰다.
- A1 재고가 생기면 옮기기 쉽게 해 두었다: 서버 빌드는 linux-arm64·amd64를 함께 만들고([release-server.yml](.github/workflows/release-server.yml)), [setup-server.sh](deploy/setup-server.sh)는 아키텍처를 보고 MediaMTX를 고른다. 새 VM에서 같은 절차를 밟고 A 레코드만 바꾸면 된다.

**백엔드: 배포 설정과 상태 확인 경로**

- **상태 확인** `GET /healthz`([health.rs](packages/api/src/health.rs)): DB에 `SELECT 1`이 되면 200, 안 되면 503. MediaMTX Control API 연결 상태(`ok`/`unreachable`/`error`)도 싣지만 그 때문에 503이 되지는 않는다. 자동 테스트 [tests/health_http.rs](packages/api/tests/health_http.rs).
- **서비스 주소·비밀 값은 환경 설정**: 서버의 `/etc/streamapp/streamapp.env`(권한 640, 저장소에는 자리 표시만 든 [deploy/streamapp.env.example](deploy/streamapp.env.example))를 앱과 MediaMTX의 systemd 유닛이 읽는다. ⑥에서 더한 값:

  | 환경변수 | 로컬 기본값 | 배포 값 |
  | --- | --- | --- |
  | `STREAMAPP_HLS_BASE_URL` | `http://localhost:8888` | `https://smj0617.com/hls`(라이브 재생 주소 `playback_url`의 앞부분. 웹·데스크톱이 이 값으로 재생) |
  | `STREAMAPP_OBS_SERVER` | `rtmp://localhost/` | `rtmp://smj0617.com/`(내 채널 설정의 OBS 서버 칸) |
  | `STREAMAPP_SECURE_COOKIE` | 꺼짐 | `true`(세션 쿠키에 `Secure`) |
  | `STREAMAPP_MEDIAMTX_WEBRTC` | `http://127.0.0.1:8889` | 같음(휴대폰 방송 WHIP 중계) |
  | `IP` / `PORT` | `127.0.0.1` / `8080` | 같음(앱은 서버 안에서만 듣고 Caddy만 바깥에 연다) |
  | `MTX_HLSALLOWORIGINS` | 설정 파일 값 | `https://smj0617.com` |
  | `MTX_WEBRTCADDITIONALHOSTS` | `[]` | `129.225.178.57`(휴대폰 ICE에 알릴 공인 IP) |

  `STREAMAPP_DEFAULT_PASSWORD`(기본 사용자 비밀번호)는 서버에서 무작위 값으로 바꿨고, 토스 시크릿 키 같은 비밀 값과 함께 저장소에 넣지 않는다.
- **배포 구성**: [deploy/](deploy/) — Caddyfile, 리눅스용 [mediamtx.yml](deploy/mediamtx.yml)(HLS·WebRTC HTTP·Control API는 127.0.0.1에서만 듣고 ICE 8189와 RTMP 1935만 바깥에), systemd 유닛([streamapp.service](deploy/systemd/streamapp.service), [mediamtx.service](deploy/systemd/mediamtx.service), 전용 사용자 `streamapp`, `Restart=always`), 서버 최초 준비 [setup-server.sh](deploy/setup-server.sh), 배포 [deploy.sh](deploy/deploy.sh)(릴리스 폴더에 풀고 링크를 바꾼 뒤 `/healthz`가 30초 안에 200이 아니면 이전 릴리스로 되돌린다).
- **재현 절차**(자세히는 [docs/deploy.md](docs/deploy.md)):

  ```bash
  # 0) OCI 콘솔: 위 표대로 VM 만들기, 보안 목록 Ingress 추가 / Cloudflare: A 레코드(DNS 전용)
  # 1) 서버 준비(한 번)
  scp -r deploy ubuntu@129.225.178.57:~/streamapp-deploy
  ssh ubuntu@129.225.178.57 'sudo bash ~/streamapp-deploy/setup-server.sh --domain smj0617.com --public-ip 129.225.178.57'
  ssh ubuntu@129.225.178.57 'sudo nano /etc/streamapp/streamapp.env'   # 기본 비밀번호, (선택) 토스 테스트 키
  # 2) 빌드 묶음 받기(태그 릴리스 또는 Actions 아티팩트) 후 배포
  gh release download -p 'streamapp-server-*-linux-amd64.tar.gz'
  bash deploy/deploy.sh --host ubuntu@129.225.178.57 --tarball streamapp-server-<버전>-linux-amd64.tar.gz
  # 3) 확인
  curl -sS https://smj0617.com/healthz
  ```

**데스크톱 클라이언트**

- 코드 [packages/desktop](packages/desktop): Rust + **Dioxus 0.7.10 desktop**(웹·서버와 같은 워크스페이스). 홈에서 배포 백엔드의 **라이브 목록**(`GET /api/live-channels`)과 **녹화방송 목록**(`GET /api/recordings`)을 보고, 카드를 누르면 재생한다. 라이브는 `playback_url`의 HLS(macOS는 WKWebView 기본 재생, Windows는 앱에 넣은 hls.js 1.7.4 + 앱 안 중계), 녹화방송은 mp4(`/media/recordings/…`, 건너뛰기 가능)다. 기본 서버 주소는 빌드할 때 `STREAMAPP_SERVER_URL`(릴리스는 저장소 변수 `https://smj0617.com`)로 넣고, 앱의 "서버 설정"에서 바꿀 수 있다. **로그인·로그아웃과 댓글**도 된다: 상단 "로그인"에서 닉네임·비밀번호로 로그인하면(`POST /api/auth/login`, 회원가입은 "웹에서 가입" 링크) 녹화방송·라이브 시청 화면 아래에서 댓글을 보고 쓰며 자기 댓글을 수정·삭제한다(`/api/comments`, 서버가 401·403·400으로 다시 확인하고 그 사유를 앱에 보인다). 세션 토큰만 앱 데이터 폴더의 `session.txt`에 서버 주소와 함께 저장해 다시 켜도 로그인이 유지되고(비밀번호는 저장하지 않음), 로그아웃하거나 서버 주소를 바꾸면 지운다(자세히는 [docs/desktop-client.md](docs/desktop-client.md)). 채팅·후원은 범위 밖이고 "웹에서 열기"로 웹 앱을 연다.
- 빌드·패키징: [packages/desktop/Dioxus.toml](packages/desktop/Dioxus.toml)(식별자 `kr.streamapp.desktop`, 아이콘, WebView2 부트스트래퍼)과 [.github/workflows/release-desktop.yml](.github/workflows/release-desktop.yml). `v*` 태그를 푸시하면 [release.yml](.github/workflows/release.yml)이 서버 묶음과 함께 아래 파일을 만들어 GitHub Release 하나에 올린다. 로컬에서는 `dx bundle --package desktop --desktop --release --package-types nsis --package-types msi`(Windows).

  | 파일 | OS·아키텍처 | 형식 |
  | --- | --- | --- |
  | `StreamApp-windows-x64-setup.exe` | Windows 10·11 x64 | NSIS 설치 프로그램(현재 사용자에게 설치, 관리자 권한 불필요) |
  | `StreamApp-windows-x64.msi` | Windows 10·11 x64 | Windows Installer |
  | `StreamApp-macos-arm64.dmg` | macOS 11 이상 Apple Silicon(arm64) | 디스크 이미지(.app, 임시 서명) |
  | `StreamApp-macos-x64.dmg` | macOS 11 이상 Intel(x86_64) | 디스크 이미지(.app, 임시 서명, Apple Silicon 러너에서 교차 빌드) |

- 다운로드 페이지 https://smj0617.com/download ([views/download.rs](packages/web/src/views/download.rs), 상단 바 "다운로드"): OS별 링크(`https://github.com/smj0617/StreamApp/releases/latest/download/<파일 이름>`), 지원 아키텍처, 설치·실행 방법을 보여 준다.

**OS별 설치·실행 확인 절차**

1. 다른 PC나 휴대폰 테더링 같은 **외부 네트워크**에서 https://smj0617.com 을 연다. 주소창에 자물쇠(유효한 인증서)가 보이고, http://smj0617.com 은 https로 넘어간다. https://smj0617.com/healthz 가 `"status":"ok"`다.
2. https://smj0617.com/download 에서 OS에 맞는 파일을 내려받는다.
3. **Windows**: `StreamApp-windows-x64-setup.exe` 실행 → SmartScreen("Windows의 PC 보호")이 뜨면 "추가 정보 → 실행" → 시작 메뉴의 StreamApp 실행.
4. **macOS**: 칩에 맞는 `.dmg`를 열어 StreamApp을 응용 프로그램으로 끌어 놓기 → Finder에서 Control-클릭 → 열기(macOS 15 이상은 시스템 설정 → 개인정보 보호 및 보안 → 그래도 열기). "손상되었기 때문에 열 수 없습니다"가 뜨면 `xattr -dr com.apple.quarantine /Applications/StreamApp.app`.
5. 앱 오른쪽 위에 `https://smj0617.com`이 보이고, 홈에 라이브·녹화방송 목록이 뜬다. 녹화방송 카드를 눌러 영상이 재생되고 막대로 건너뛸 수 있는지, (방송 중인 채널이 있으면) 라이브 카드를 눌러 영상이 나오는지 본다. 라이브가 없으면 웹에서 로그인해 "휴대폰 방송"이나 OBS(내 채널 설정의 서버·스트림 키)로 송출을 켜고 확인한다.

**테스트한 OS·아키텍처와 결과**

2026-11-21, 배포 서버 `https://smj0617.com`(커밋 `78ea65d` 서버 빌드, 이후 Release `v0.6.0` 빌드로 다시 배포)에 붙여 확인했다. 데스크톱 패키지는 GitHub Actions 실행 [44890524324](https://github.com/smj0617/StreamApp/actions/runs/44890524324)(커밋 `68a24f4`, 기본 서버 `https://smj0617.com`)의 산출물이다.

| 대상 | OS·아키텍처 | 패키지 | 결과 |
| --- | --- | --- | --- |
| 도메인·HTTPS | 이 PC(Windows 11, 국내 가정용 인터넷 → 오사카 VM) | — | 성공: Let's Encrypt 인증서(`CN=smj0617.com`, 발급 YE1, 만료 2027-02-19), http → https 308, `/healthz` 200(`database`·`mediamtx` ok), `/internal/mediamtx/auth` 403 |
| 웹 기능 | 같음(Chrome 계열 브라우저) | — | 성공: 로그인(세션 쿠키 `Secure`·`HttpOnly`), 녹화방송 올리기 201·비로그인 401·영상 Range 206, OBS 방식 시험 송출(RTMP 1935) → 홈 라이브 카드 → `/live/<채널>`에서 HLS 1280×720 재생(재생 시간이 계속 늘어남) |
| 데스크톱 | **Windows 11 Home 26200, x64** | `StreamApp-windows-x64-setup.exe`(5.8MB) | 성공: `/S`로 현재 사용자에게 설치(`%LOCALAPPDATA%\Programs\StreamApp`) → 실행 → 오른쪽 위 `https://smj0617.com`, 홈에 라이브 1건·녹화방송 1건 → 라이브 재생(hls.js + 앱 중계, 송출 화면의 시각이 흐름) → 녹화방송 mp4 로드(0:12)·8초 지점으로 건너뛰기 |
| 데스크톱 | **macOS 26.7.2, arm64**(GitHub Actions `macos-latest` 러너) | `StreamApp-macos-arm64.dmg`(4.9MB) | 부분 확인: 워크플로가 dmg를 붙여 앱을 실행하고 20초 뒤 화면을 캡처(실행 결과의 `smoke-macos-arm64` 아티팩트) → 오른쪽 위 `https://smj0617.com`, 라이브 1건("철수님의 라이브")·녹화방송 1건 목록 표시. **실제 Mac이 없어** 카드를 눌러 재생하는 것과 Gatekeeper 첫 실행 경고는 확인하지 못했다 |
| 데스크톱 | macOS x86_64(Intel) | `StreamApp-macos-x64.dmg`(5.3MB) | 빌드만 확인(교차 빌드 성공). 실행은 확인하지 못했다 |
| 데스크톱 | Windows x64 | `StreamApp-windows-x64.msi`(4.3MB) | 빌드만 확인. 설치는 setup.exe로만 확인했다 |
| 휴대폰 송출 | Galaxy S24(5G), 휴대폰 브라우저 → https://smj0617.com/broadcast | — | 성공: 로그인 → 카메라 켜기·권한 허용 → 방송 시작("방송 중") → PC 브라우저 홈의 LIVE 카드에서 휴대폰 세로 영상 재생 → 휴대폰에서 방송 종료 → 시청 화면에 "라이브가 종료되었습니다"와 "종료" 표시. 영상은 휴대폰에서 VM의 ICE 8189로 바로 간다(`MTX_WEBRTCADDITIONALHOSTS`=공인 IP) |
| 데스크톱 로그인·댓글(v0.6.1) | Windows 11 Home 26200, x64, Release `v0.6.1` setup.exe → 배포 서버(0.6.1) | `StreamApp-windows-x64-setup.exe` | 성공: 비로그인 녹화방송 화면에 "로그인하면 댓글을 쓸 수 있습니다" → 영희로 로그인(상단 "영희 / 로그아웃") → 댓글 쓰기("댓글 1", 내 댓글에 수정·삭제, 웹 API `GET /api/recordings/1/comments`에도 보임) → 삭제(서버 0건) → 로그아웃(비로그인 안내, `session.txt` 삭제) |

- 이번 단계에서 하지 않은 것: Apple 개발자 인증서 서명·공증과 Windows 코드 서명(처음 실행 때 OS 경고가 뜬다), 앱 자동 업데이트, Windows ARM64·Linux 데스크톱 패키지, 데스크톱 앱 안의 회원가입·채팅·후원, 세션 토큰의 OS 자격 증명 저장소 보관, TURN 서버(대칭형 NAT 망에서 휴대폰 송출).

## 현황 (2026-11-06)

### 과제 ④: 도네이션 — 구현

- 자동 테스트 180개 통과(후원 앱 서비스 21개, 결제 서비스·웹훅 HTTP 7개 추가), clippy·fmt·wasm 빌드 통과.
- 실제 토스페이먼츠(문서용 주문서형 테스트 키)로 Chrome에서 확인: 영희 로그인 → 녹화방송 상세 "5,000원 후원하기" → 토스 결제수단·약관 UI("테스트 환경이에요" 표시) → 신용·체크카드 → 국민카드 → "5,000원 결제하기" → 토스 테스트 결제창(KB 화면) 열림 → 결제창 닫기 시 결제 UI 유지 → "취소"로 주문 취소("결제를 취소했습니다. 후원하지 않았습니다.", 합계 0원). 서버의 시크릿 키 인증도 실제 승인 API로 확인했다(가짜 결제 키는 "결제 진행 데이터가 존재하지 않습니다"로 거절, 확정 안 됨).
- 이전 단계 DB 복사본으로 확인: 비로그인 주문 401, 금액 조작 승인 요청 400(결제 서비스 호출 없음), 500원 주문 400, 실패 주소에서 주문이 `Failed`(사유 표시)로 닫힘, 철수(주인)에게는 후원 칸 대신 "내 방송에는 후원할 수 없습니다"와 주문 요청 400.
- 확인하지 못한 것: 카드사 인증을 끝낸 실제 승인 성공과 그 뒤의 새로고침·웹훅 중복(카드사 인증 화면이 휴대폰 번호·주민등록번호 로그인을 요구해 자동화로 진행하지 않음, 가짜 결제 서비스·가짜 토스 서버 테스트로만 확인), 실제 웹훅 수신, 휴대폰 결제창, 라이브 시청 페이지의 후원 칸.

### 과제 ③: 인증·인가와 댓글 — 구현

- 자동 테스트 152개 통과(로그인·세션 13개, 댓글 10개, 녹화방송 주인 권한 9개, 송출 종료 권한·세션 쿠키 HTTP 테스트 추가), clippy·fmt·wasm 빌드 통과.
- 브라우저에서 확인(이전 단계 DB 복사본으로): 앱 시작 때 이전 사용자 3명에 기본 비밀번호 채우기 → 비로그인 댓글·제목 수정·삭제·내 채널 조회 401 → 틀린 비밀번호 거절 → 영희 로그인 → 철수 녹화방송에 관리 칸 없음, 제목 수정·삭제 요청 403, 없는 번호 404 → 영희 댓글 쓰기 → 로그아웃 → 새 사용자 회원가입(자동 로그인) → 영희 댓글 수정·삭제 403 → 철수 로그인 → 관리 칸에서 제목 수정 → 시험 송출 라이브 시청 페이지에서 라이브 댓글 쓰기·표시.
- 확인하지 못한 것: 화면에서 녹화방송 삭제 버튼 누르기(서비스 테스트로만 확인), 실제 휴대폰으로 로그인 후 송출(WHIP 중계는 HTTP 테스트로 확인), 댓글 수정 화면 저장 버튼.


### 과제 ②: 휴대폰 라이브 송출 — 완료 (2026-11-04)

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
├── api/                 # 서버 함수, 앱 서비스(Dioxus 비의존), MediaMTX 연동·녹화방송 파일·휴대폰 방송 중계·결제 웹훅 axum Router, 토스페이먼츠 클라이언트
│   ├── migrations/      # 0001 사용자·채널, 0002 라이브, 0003 녹화방송, 0004 비밀번호·세션·댓글, 0005 후원, 0006 휴대폰 송출 세션 (기존 파일은 수정하지 말고 새 번호로 추가)
│   └── tests/           # 앱 서비스(메모리 SQLite, 가짜 결제 서비스), MediaMTX 연동·녹화방송 파일·휴대폰 방송 중계·결제 서비스 HTTP 테스트
└── web/                 # 화면: 홈, 시청 페이지(후원·댓글), 휴대폰 방송, 녹화방송 등록·상세(관리·후원·댓글), 후원 결과, 내 채널 설정, 로그인·회원가입
scripts/                 # dev.ps1, test-publish.ps1, k8s-up.ps1·k8s-down.ps1·k8s-watch.ps1·load-test.ps1(⑤ 로컬 오토스케일)
Dockerfile               # 서버 컨테이너 이미지(⑤), .dockerignore
k8s/                     # ⑤ kind 클러스터 설정, metrics-server, app/(앱·HPA·MediaMTX·Redis·공유 볼륨 매니페스트)
loadtest/                # ⑤ k6 부하 스크립트(결과는 loadtest/out/, git 제외)
tools/mediamtx/          # MediaMTX v1.21.3 설정 (실행 파일은 dev.ps1이 .tools\로 내려받음)
```

## 로컬에서 실행하기 (Windows)

필요한 도구: Visual Studio Build Tools(C++), Rust(stable, MSVC) + `wasm32-unknown-unknown` 타깃, `dx` 0.7.10, ffmpeg. `dev.ps1`이 시작할 때 확인하고 없으면 무엇을 설치할지 알려 준다.

```powershell
# 앱 + MediaMTX 실행 (기본 포트 8080, 이 PC에서는 다른 프로그램이 8080을 써서 8090으로 띄운다)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1 -Port 8090
```

- 브라우저에서 `http://localhost:<포트>`를 연다. 오른쪽 위 "로그인"으로 로그인하거나 "회원가입"한다(빈 DB면 철수·영희·민수가 비밀번호 `test1234`로 미리 만들어진다).
- **휴대폰 송출**: 위 "② 인증·인가 없는 휴대폰 라이브 송출"의 접속 환경과 절차를 따른다.
- **OBS 송출**: 내 채널 설정의 "서버"·"스트림 키" 값을 OBS의 설정 → 방송 → 서비스 "사용자 지정"에 넣고 방송을 시작한다. OBS 없이 테스트하려면:

  ```powershell
  .\scripts\test-publish.ps1 -ChannelId <채널 ID> -StreamKey <송출 키>
  ```

- **두 사용자로 확인**: 로그인 세션은 쿠키라서, 일반 창과 시크릿 창을 함께 쓰거나 한 브라우저에서 `localhost`와 `127.0.0.1`로 주소를 나눠 열면 서로 다른 사용자로 로그인할 수 있다.
- **초기화**: 앱을 끄고 `data/` 폴더(`streamapp.db`와 `recordings/`)를 지우면 기본 사용자 3명만 있는 상태로 돌아간다.

### 환경변수

모두 선택 사항이다. 정하지 않으면 아래 기본값을 쓴다(경로는 앱을 실행한 폴더 기준, `dev.ps1`은 레포 루트). 세션 서명 키 같은 비밀 값은 필요 없다(세션은 DB에 둔다). 기본 비밀번호는 로컬 확인용 시험 값이며, 실제 비밀번호를 여기에 적어 커밋하지 않는다. 후원 결제(④)는 토스페이먼츠 **테스트 키**를 넣어야 켜진다. 키는 개발자센터에서 각자 확인해 셸에서만 정하고 저장소에 적지 않는다(아래 키 값은 자리 표시다).

```powershell
$env:STREAMAPP_DB = "data/streamapp.db"                  # SQLite DB 파일
$env:STREAMAPP_RECORDINGS_DIR = "data/recordings"        # 녹화방송 영상 파일 폴더
$env:STREAMAPP_MEDIAMTX_API = "http://127.0.0.1:9997"    # MediaMTX Control API(앱 시작 시 송출 상태 맞추기)
$env:STREAMAPP_DEFAULT_PASSWORD = "test1234"             # 빈 DB의 기본 사용자와 이전 단계(비밀번호 없는) 사용자에게 넣을 시험용 비밀번호(8~64자)
$env:STREAMAPP_TOSS_CLIENT_KEY = "test_ck_..."           # 토스페이먼츠 API 개별 연동 테스트 클라이언트 키(브라우저 결제창용). 없으면 후원 결제를 끈다
$env:STREAMAPP_TOSS_SECRET_KEY = "test_sk_..."           # 토스페이먼츠 테스트 시크릿 키(서버의 결제 승인·조회용, 비밀 값). test_로 시작하지 않으면 받지 않는다
# $env:STREAMAPP_TOSS_API = "https://api.tosspayments.com" # 결제 서비스 API 주소(기본값 그대로 둔다)
# $env:STREAMAPP_REDIS_URL = "redis://127.0.0.1:6379"   # (⑤ 여러 인스턴스) 채팅을 다른 인스턴스로 전달할 Redis. 없으면 메모리만
# $env:STREAMAPP_MULTI_INSTANCE = "true"                 # (⑤ 여러 인스턴스) 시작 때 MediaMTX에 닿지 못해도 라이브를 닫지 않는다
# $env:STREAMAPP_INSTANCE_NAME = "local-1"               # (⑤) 응답 헤더 x-streamapp-instance에 실을 인스턴스 이름
```

## 테스트

PR과 `main` 푸시마다 GitHub Actions([.github/workflows/ci.yml](.github/workflows/ci.yml))가 아래 네 가지를 돌린다. Rust 버전은 [rust-toolchain.toml](rust-toolchain.toml)로 로컬과 CI가 같게 고정한다.

```powershell
cargo test --workspace --features api/server,web/server
cargo check -p web --target wasm32-unknown-unknown --no-default-features --features web
cargo clippy --workspace --all-targets --features api/server,web/server -- -D warnings
cargo fmt --all --check
```

과제 ⑤의 Redis 채팅 중계 통합 테스트는 Redis가 있어야 해서 기본으로 건너뛴다(`#[ignore]`). Docker로 Redis를 띄워 돌린다.

```powershell
docker run -d --rm -p 6379:6379 --name streamapp-test-redis redis:8.8.4-alpine
$env:STREAMAPP_TEST_REDIS_URL = "redis://127.0.0.1:6379"
cargo test -p api --features server --test multi_instance -- --include-ignored
docker stop streamapp-test-redis
```

컨테이너 이미지와 쿠버네티스 매니페스트는 [.github/workflows/container.yml](.github/workflows/container.yml)이 관련 파일이 바뀐 PR에서 확인한다(이미지 빌드, `kubectl kustomize k8s/app`·`k8s/metrics-server`). 로컬에서는 `docker build -t streamapp:local .`와 `kubectl kustomize k8s/app`(먼저 `secret.env.example`을 `secret.env`로 복사).

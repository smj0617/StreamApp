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

## 현황 (2026-11-05)

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
├── api/                 # 서버 함수, 앱 서비스(Dioxus 비의존), MediaMTX 연동·녹화방송 파일·휴대폰 방송 중계 axum Router
│   ├── migrations/      # 0001 사용자·채널, 0002 라이브, 0003 녹화방송, 0004 비밀번호·세션·댓글 (기존 파일은 수정하지 말고 새 번호로 추가)
│   └── tests/           # 앱 서비스(메모리 SQLite), MediaMTX 연동·녹화방송 파일·휴대폰 방송 중계 HTTP 테스트
└── web/                 # 화면: 홈, 시청 페이지, 휴대폰 방송, 녹화방송 등록·상세(관리·댓글), 내 채널 설정, 로그인·회원가입
scripts/                 # dev.ps1, test-publish.ps1
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

모두 선택 사항이다. 정하지 않으면 아래 기본값을 쓴다(경로는 앱을 실행한 폴더 기준, `dev.ps1`은 레포 루트). 세션 서명 키 같은 비밀 값은 필요 없다(세션은 DB에 둔다). 기본 비밀번호는 로컬 확인용 시험 값이며, 실제 비밀번호를 여기에 적어 커밋하지 않는다.

```powershell
$env:STREAMAPP_DB = "data/streamapp.db"                  # SQLite DB 파일
$env:STREAMAPP_RECORDINGS_DIR = "data/recordings"        # 녹화방송 영상 파일 폴더
$env:STREAMAPP_MEDIAMTX_API = "http://127.0.0.1:9997"    # MediaMTX Control API(앱 시작 시 송출 상태 맞추기)
$env:STREAMAPP_DEFAULT_PASSWORD = "test1234"             # 빈 DB의 기본 사용자와 이전 단계(비밀번호 없는) 사용자에게 넣을 시험용 비밀번호(8~64자)
```

## 테스트

PR과 `main` 푸시마다 GitHub Actions([.github/workflows/ci.yml](.github/workflows/ci.yml))가 아래 네 가지를 돌린다. Rust 버전은 [rust-toolchain.toml](rust-toolchain.toml)로 로컬과 CI가 같게 고정한다.

```powershell
cargo test --workspace --features api/server,web/server
cargo check -p web --target wasm32-unknown-unknown --no-default-features --features web
cargo clippy --workspace --all-targets --features api/server,web/server -- -D warnings
cargo fmt --all --check
```

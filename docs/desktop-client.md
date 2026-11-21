# 데스크톱 클라이언트 (과제 ⑥)

Windows·macOS에서 배포한 StreamApp 백엔드의 라이브·녹화방송 목록을 보고 재생하고, 로그인하면 시청 화면에서 댓글을 쓰는 앱이다.
Rust + Dioxus 0.7.10 desktop(웹뷰: Windows WebView2, macOS WKWebView)으로 만들었고 코드는 [`packages/desktop`](../packages/desktop)에 있다.
이 문서는 README에 옮길 초안을 겸한다.

## 범위

| 화면 | 쓰는 API | 비고 |
|---|---|---|
| 홈: 라이브 목록 | `GET /api/live-channels` | 카드: 제목·채널 이름·시작 후 경과 시간 |
| 홈: 녹화방송 목록 | `GET /api/recordings` | 카드: 제목·채널 이름·등록 후 지난 시간 |
| 라이브 시청 | `GET /api/watch/{channel_id}` → HLS | 제목·채널 이름, 오프라인 안내 |
| 녹화방송 시청 | `GET /api/recordings/{id}` → mp4 (`/media/recordings/<파일>`) | 제목·채널 이름 |
| 로그인 | `POST /api/auth/login` `{nickname, password}` | 상단 오른쪽 **로그인** 링크. 회원가입은 **웹에서 가입** 링크(웹 `/signup`) |
| 로그인 상태·로그아웃 | `GET /api/current-user`, `POST /api/auth/logout` | 상단 오른쪽에 닉네임과 **로그아웃** |
| 댓글(녹화방송·라이브 시청 아래) | `GET /api/recordings/{id}/comments`, `GET /api/lives/{live_id}/comments` | 목록은 누구나. 라이브 회차 ID는 `GET /api/watch/{channel_id}`의 `live.id`. 오프라인이면 라이브 댓글 칸을 숨긴다 |
| 댓글 쓰기·수정·삭제 | `POST /api/comments` `{target: {"Recording": id} \| {"Live": id}, content}`, `POST /api/comments/edit` `{id, content}`, `POST /api/comments/delete` `{id}` | 로그인해야 쓴다(아니면 "로그인하면 댓글을 쓸 수 있습니다"와 로그인 링크). 내용 1~500자. 자기 댓글에만 수정·삭제. 성공하면 목록을 다시 불러온다 |
| 서버 설정 | – | 서버 주소 바꾸기·저장·기본값으로 |

채팅·후원은 범위 밖이다. 시청 화면의 **웹에서 열기**가 기본 브라우저로 웹 앱의 같은 화면을 연다.

서버 함수(`#[get]`)는 평범한 HTTP GET에 JSON으로 답한다. 데스크톱은 `reqwest`(rustls)로 부르고, 응답 타입은
`api::dto`에서 읽는 필드만 옮겨 적었다(모르는 필드는 무시). 서버 dto와 맞는지는 `packages/desktop/src/client.rs`의
테스트가 `api::dto` 값을 직렬화해 확인한다(`api`는 dev-dependency로만 쓴다).

서버가 거절하면(401 로그인 필요, 403 작성자 아님, 400 내용 규칙, 404 없음) 응답 본문
`{"message":…,"code":401,"data":{"ServerError":{"message":"로그인이 필요합니다","code":401}}}`에서
`data.ServerError.message`를 꺼내 웹과 같은 문구를 보인다(`client.rs`의 `rejection_message`). 401이면 로그인이 끝났을 수
있으니 다시 로그인하라고 덧붙인다. 버튼을 숨기는 것은 화면 편의이고 권한은 서버가 확인한다.

### 로그인 세션

- 서버는 로그인 응답에 `streamapp_session` 쿠키(HttpOnly, SameSite=Lax, 30일, 배포에서는 Secure)로 세션 토큰을 준다.
  데스크톱은 reqwest의 쿠키 저장소를 쓰지 않고, 로그인 응답의 `Set-Cookie`에서 토큰만 꺼내 들고 있다가 로그인이 필요한
  요청에 `Cookie: streamapp_session=<토큰>`으로 싣는다(`client.rs`). 그래서 토큰은 받은 서버에만 가고, 목록·영상·HLS 중계
  요청에는 실리지 않는다.
- 다음 실행에도 로그인을 유지하려고 토큰을 **서버 주소와 함께** 파일에 저장한다(`packages/desktop/src/session.rs`).
  비밀번호는 저장하지 않는다.
  - Windows `%APPDATA%\StreamApp\session.txt`, macOS `~/Library/Application Support/StreamApp/session.txt`
    (서버 주소 파일과 같은 폴더). 내용은 첫 줄 서버 주소, 둘째 줄 토큰. macOS(Unix)에서는 소유자만 읽고 쓰게(0600) 만든다.
  - 시작할 때 저장한 서버 주소가 지금 서버와 같을 때만 쓰고 `GET /api/current-user`로 확인한다. 세션이 끝났으면
    (`null`) 파일을 지운다. 연결에 실패하면 세션은 두고 "로그인" 링크를 보인다.
  - **서버 설정**에서 주소를 바꾸면(기본값으로 포함) 이전 서버의 세션을 버리고 파일을 지운다(새 서버에 이전 토큰을 보내지 않는다).
    같은 서버라도 주소 문자열이 다르면(`localhost`와 `127.0.0.1`) 다른 서버로 본다. 이때 이전 서버의 세션은 서버에 기한까지 남지만
    토큰을 가진 곳이 없다.
  - **로그아웃**하면 서버 세션을 지우고(`POST /api/auth/logout`) 파일도 지운다. 서버에 연결하지 못해도 파일은 지운다.
- **위험**: 이 파일을 읽을 수 있는 사람이나 프로그램(같은 OS 계정으로 도는 프로그램 포함)은 세션 기한인 30일 동안 그 계정으로
  댓글 쓰기·수정·삭제 등 로그인한 사용자가 하는 일을 할 수 있다. 파일은 OS 사용자 폴더 안에 있어 다른 OS 계정은 기본적으로
  읽지 못한다. 공용 PC에서는 다 쓴 뒤 **로그아웃**한다(서버 세션이 지워져 파일이 남아 있어도 쓸 수 없다).
  OS 자격 증명 저장소(Windows 자격 증명 관리자, macOS 키체인)에 넣는 방법도 있으나, 서버 주소 파일과 같은 방식으로 단순하게 두었다.
- 로컬 개발 서버(`http://`)에서는 토큰이 암호화되지 않은 HTTP로 간다. 배포 서버(`https://`)에서만 쓰는 것을 전제로 한다.

### 서버 주소

- 기본값: 빌드할 때 환경변수 `STREAMAPP_SERVER_URL`(`option_env!`). 없거나 비었으면 `http://localhost:8090`.
- 실행 중: 상단 **서버 설정**에서 바꾼다. 저장하면 다음 실행에도 쓴다.
  - Windows `%APPDATA%\StreamApp\server-url.txt`
  - macOS `~/Library/Application Support/StreamApp/server-url.txt`
  - **기본값으로**를 누르면 파일을 지우고 빌드 값으로 돌아간다.

### 재생

- 라이브 재생 주소는 `GET /api/watch/{channel_id}`의 `playback_url`을 쓴다. 전체 주소면 그대로, `/`로 시작하는 경로면
  서버 주소 기준으로 바꾼다. 비어 있으면 배포 규칙 `<서버>/hls/<채널ID>/index.m3u8`을 쓴다.
- **macOS**: WKWebView가 HLS를 직접 재생하므로 `<video src>`에 재생 주소를 넣는다.
- **Windows**: WebView2는 HLS를 직접 재생하지 못하므로 앱에 넣어 둔 hls.js 1.7.4(`packages/desktop/assets/hls.min.js`,
  Apache-2.0, 웹과 같은 버전)로 재생한다. 웹뷰 페이지의 출처(`http://dioxus.index.html`)가 HLS 서버의 허용 출처
  (`hlsAllowOrigins`)에 없어 CORS에 막히므로, hls.js는 앱 안의 중계 주소 `/hlsproxy/...`를 받고 앱(Rust)이 원래 HLS
  주소에서 가져와 넘긴다(`packages/desktop/src/hls_proxy.rs`). 그래서 서버 쪽 CORS 설정을 바꿀 필요가 없다.
- 녹화방송 mp4는 `<video src>`가 서버에 바로 요청한다(미디어 요소는 CORS를 보지 않고, Range 요청도 서버가 처리한다).

## 로컬에서 실행·빌드

필요한 것: rust-toolchain.toml의 Rust, dx 0.7.10(`cargo binstall dioxus-cli@0.7.10`), Windows는 WebView2 런타임(Windows 11 기본).

```powershell
# 1) 백엔드 띄우기(다른 터미널)
.\scripts\dev.ps1 -Port 8090

# 2) 데스크톱 앱 개발 실행
dx serve --package desktop --desktop

# 3) 설치 파일 만들기(Windows: NSIS .exe와 .msi)
$env:STREAMAPP_SERVER_URL = "https://<도메인>"   # 앱의 기본 서버 주소. 빼면 http://localhost:8090
dx bundle --package desktop --desktop --release --package-types nsis --package-types msi
# → packages\desktop\dist\StreamApp_<버전>_x64-setup.exe, StreamApp_<버전>_x64.msi
```

macOS에서는 `--package-types macos`(→ `StreamApp.app`)나 `--package-types dmg`를 쓴다. 릴리스 워크플로는 `.app`을 만든 뒤
임시(ad-hoc) 서명을 하고 `hdiutil`로 `.dmg`를 직접 만든다.

번들 설정은 [`packages/desktop/Dioxus.toml`](../packages/desktop/Dioxus.toml)에 있다.

- 식별자 `kr.streamapp.desktop`, 앱 이름 `StreamApp`(실행 파일 이름 `[[bin]] name = "StreamApp"`에서 온다), 아이콘 `packages/desktop/icons/`.
- Windows 설치 파일은 WebView2가 없는 PC에서만 작은 부트스트래퍼로 받아 설치한다(`EmbedBootstrapper`). dx 기본값(오프라인
  설치 파일 포함)은 설치 파일이 240MB를 넘는다.
- NSIS는 현재 사용자에게 설치한다(관리자 권한 불필요).
- `short_description`·`long_description`은 영문으로 둔다. makensis가 dx가 만든 스크립트를 시스템 코드 페이지로 읽어 한글이 있으면 실패한다.

## 릴리스(태그)

[`.github/workflows/release-desktop.yml`](../.github/workflows/release-desktop.yml)

1. 저장소 **Settings → Secrets and variables → Actions → Variables**에 `STREAMAPP_SERVER_URL`(예: `https://<도메인>`)을 만든다.
   비어 있으면 워크플로가 경고를 남기고 기본값 `http://localhost:8090`으로 빌드한다.
2. 버전을 올리고(`Cargo.toml`의 `workspace.package.version`) 태그를 푸시한다.
   ```powershell
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. 세 러너가 번들하고, 끝나면 GitHub Release에 아래 고정 이름으로 올린다.

| 파일 | 러너 | 대상 |
|---|---|---|
| `StreamApp-windows-x64-setup.exe` | windows-latest | x86_64-pc-windows-msvc (NSIS) |
| `StreamApp-windows-x64.msi` | windows-latest | x86_64-pc-windows-msvc (WiX) |
| `StreamApp-macos-arm64.dmg` | macos-latest | aarch64-apple-darwin |
| `StreamApp-macos-x64.dmg` | macos-latest(교차 빌드) | x86_64-apple-darwin |

Actions 탭에서 손으로 돌리면(workflow_dispatch) 릴리스 없이 실행 결과의 Artifacts로만 받는다.
Intel Mac용은 Apple Silicon 러너에서 `--target x86_64-apple-darwin`으로 교차 빌드한다(Intel macOS 러너는 줄어드는 중이라 쓰지 않았다).

## 다운로드 페이지

웹 앱의 `/download`([`packages/web/src/views/download.rs`](../packages/web/src/views/download.rs), 상단 바 **다운로드**).

- 링크: `https://github.com/smj0617/StreamApp/releases/latest/download/<파일 이름>`. 가장 최근 릴리스의 같은 이름 파일을
  가리키므로 새 버전을 올려도 페이지를 고치지 않는다. 앞부분은 상수 `DOWNLOAD_BASE_URL`이고, 웹을 빌드할 때 환경변수
  `STREAMAPP_DOWNLOAD_BASE_URL`로 바꿀 수 있다. 파일 이름을 바꾸면 워크플로와 이 파일을 함께 고친다.
- 지원 아키텍처: Windows 10·11 x64, macOS 11 이상 Apple Silicon(arm64)·Intel(x86_64). Windows ARM64·Linux는 없다.

## OS별 설치·실행·확인 절차

서명하지 않은 앱이라 처음 실행할 때 OS가 막는다.

### Windows

1. `StreamApp-windows-x64-setup.exe`를 실행한다. "Windows의 PC 보호"(SmartScreen)가 뜨면 **추가 정보 → 실행**.
   (.msi도 같다.)
2. 시작 메뉴의 **StreamApp**을 실행한다.
3. 확인: 상단 오른쪽에 서버 주소가 보이고, 홈에 라이브·녹화방송 목록이 뜬다. 녹화방송 카드를 눌러 재생되는지,
   라이브 카드를 눌러 영상이 나오는지 본다. 상단 오른쪽 **로그인**으로 로그인한 뒤 녹화방송 시청 화면 아래에서 댓글을
   쓰고, 고치고, 지울 수 있는지 본다.

### macOS

1. 칩에 맞는 `.dmg`를 열고 `StreamApp`을 Applications로 끌어 놓는다.
2. Finder에서 **Control-클릭(우클릭) → 열기 → 열기**. macOS 15 이상은 한 번 실행을 시도한 뒤
   **시스템 설정 → 개인정보 보호 및 보안 → 그래도 열기**.
3. "손상되었기 때문에 열 수 없습니다"가 뜨면:
   ```sh
   xattr -dr com.apple.quarantine /Applications/StreamApp.app
   ```
4. 확인 절차는 Windows와 같다.

## 테스트 결과 기록

| 날짜 | OS | 아키텍처 | 산출물 | 실행 | 목록 | 녹화방송 재생 | 라이브 재생 | 비고 |
|---|---|---|---|---|---|---|---|---|
| 2026-11-21 | Windows 11 Home 26200 | x64 | 로컬 `dx bundle` (setup.exe 5.8MB, msi 4.3MB) | 성공(번들 안 실행 파일) | 성공(로컬 서버 8090) | 성공(mp4) | 성공(hls.js + 중계, 테스트 송출) | 설치 프로그램은 실행하지 않음 |
| 2026-11-21 | Windows 11 Home 26200 | x64 | Actions 44890524324 setup.exe(기본 서버 https://smj0617.com) | 성공(`/S` 설치 후 실행) | 성공(배포 서버) | 성공(mp4 로드, 8초로 건너뛰기) | 성공(hls.js + 중계, RTMP 시험 송출) | |
| | Windows | x64 | msi | | | | | 빌드만 확인 |
| 2026-11-21 | macOS 26.7.2(Actions `macos-latest` 러너) | arm64 | Actions 44890524324 dmg | 성공(러너에서 실행, 화면 캡처) | 성공(배포 서버, 라이브 1·녹화 1) | 미확인 | 미확인 | 실제 Mac 없음 |
| | macOS | x86_64 | dmg | | | | | 빌드만 확인(교차 빌드) |

### 자동 검사

```powershell
cargo test -p desktop                                   # 주소 처리·HLS 중계 주소·서버 dto 호환·오류 문구·세션 파일
cargo clippy -p desktop --all-targets -- -D warnings
```

CI(ubuntu)는 `--workspace`로 데스크톱 크레이트까지 컴파일하므로 WebKitGTK·GTK·libxdo 개발 패키지를 설치한다
(`.github/workflows/ci.yml`).

# 데스크톱 클라이언트 (과제 ⑥)

Windows·macOS에서 배포한 StreamApp 백엔드의 라이브·녹화방송 목록을 보고 재생하는 앱이다.
Rust + Dioxus 0.7.10 desktop(웹뷰: Windows WebView2, macOS WKWebView)으로 만들었고 코드는 [`packages/desktop`](../packages/desktop)에 있다.
이 문서는 README에 옮길 초안을 겸한다.

## 범위

| 화면 | 쓰는 API | 비고 |
|---|---|---|
| 홈: 라이브 목록 | `GET /api/live-channels` | 카드: 제목·채널 이름·시작 후 경과 시간 |
| 홈: 녹화방송 목록 | `GET /api/recordings` | 카드: 제목·채널 이름·등록 후 지난 시간 |
| 라이브 시청 | `GET /api/watch/{channel_id}` → HLS | 제목·채널 이름, 오프라인 안내 |
| 녹화방송 시청 | `GET /api/recordings/{id}` → mp4 (`/media/recordings/<파일>`) | 제목·채널 이름 |
| 서버 설정 | – | 서버 주소 바꾸기·저장·기본값으로 |

로그인·채팅·댓글·후원은 범위 밖이다. 시청 화면의 **웹에서 열기**가 기본 브라우저로 웹 앱의 같은 화면을 연다.

서버 함수(`#[get]`)는 평범한 HTTP GET에 JSON으로 답한다. 데스크톱은 `reqwest`(rustls)로 부르고, 응답 타입은
`api::dto`에서 읽는 필드만 옮겨 적었다(모르는 필드는 무시). 서버 dto와 맞는지는 `packages/desktop/src/client.rs`의
테스트가 `api::dto` 값을 직렬화해 확인한다(`api`는 dev-dependency로만 쓴다).

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
   라이브 카드를 눌러 영상이 나오는지 본다.

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
cargo test -p desktop                                   # 주소 처리·HLS 중계 주소·서버 dto 호환
cargo clippy -p desktop --all-targets -- -D warnings
```

CI(ubuntu)는 `--workspace`로 데스크톱 크레이트까지 컴파일하므로 WebKitGTK·GTK·libxdo 개발 패키지를 설치한다
(`.github/workflows/ci.yml`).

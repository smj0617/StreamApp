# StreamApp

치지직을 본뜬 라이브 방송 플랫폼. Dioxus 0.7.10 풀스택(웹)과 Cargo workspace 모노레포로 만들고, 영상은 외부 미디어 서버 MediaMTX가 맡는다. 작게 만들고 반복 개선한다.

용어는 [GLOSSARY.md](GLOSSARY.md), 설계 결정은 [docs/adr/](docs/adr/)에 있다.

## 현황 (2026-11-04)

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
- 레포에 CI가 아직 없다.

## 구조

```text
packages/
├── api/                 # 서버 함수, 앱 서비스(Dioxus 비의존), MediaMTX 연동 axum Router
│   ├── migrations/      # 0001 사용자·채널, 0002 라이브 (기존 파일은 수정하지 말고 새 번호로 추가)
│   └── tests/           # 앱 서비스(메모리 SQLite), MediaMTX 연동 HTTP 테스트
└── web/                 # 화면: 홈, 시청 페이지, 내 채널 설정, 사용자 메뉴
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
- **송출**: 내 채널 설정의 "서버"·"스트림 키" 값을 OBS의 설정 → 방송 → 서비스 "사용자 지정"에 넣고 방송을 시작한다. OBS 없이 테스트하려면:

  ```powershell
  .\scripts\test-publish.ps1 -ChannelId <채널 ID> -StreamKey <송출 키>
  ```

- **두 사용자로 채팅 확인**: 사용자 선택은 쿠키라서, 한 브라우저에서는 `localhost`와 `127.0.0.1`로 주소를 나눠 열면 서로 다른 사용자가 된다.
- **초기화**: 앱을 끄고 `data/streamapp.db`를 지우면 기본 사용자 3명만 있는 상태로 돌아간다.
- 환경 변수: `STREAMAPP_DB`(DB 경로), `STREAMAPP_MEDIAMTX_API`(Control API 주소, 기본 `http://127.0.0.1:9997`).

## 테스트

```powershell
cargo test --workspace --features api/server,web/server
cargo check -p web --target wasm32-unknown-unknown --no-default-features --features web
cargo clippy --workspace --all-targets --features api/server,web/server -- -D warnings
cargo fmt --all --check
```

# 영상은 MediaMTX가 맡고 앱은 영상을 다루지 않는다

송출 수신(RTMP)과 HLS 변환·배포는 외부 미디어 서버인 MediaMTX가 맡고, Rust 앱은 영상 바이트를 직접 다루지 않는다. 앱은 MediaMTX의 HTTP 인증 요청으로 송출 키를 확인하고, 훅으로 라이브 시작·종료를 전달받으며, 시청자 브라우저는 hls.js로 MediaMTX를 직접 재생한다. Rust 모노레포에 Go 실행 파일이 끼지만, 레포에는 설정과 실행 스크립트만 두고 바이너리는 개발 스크립트가 내려받는다.

## Considered Options

- **xiu** (순수 Rust, RTMP→HLS): workspace에 넣을 수 있지만 MediaMTX만큼 성숙한지 확인하지 못했다.
- **직접 구현**: 쓸 만한 RTMP 라이브러리(rml_rtmp)가 2023년 이후 정체돼 있고 HLS 변환까지 짜야 해서, 작게 시작한다는 목표와 맞지 않는다.

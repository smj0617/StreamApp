# 과제 ⑤ 로컬 오토스케일 확인 기록 (2026-11-21)

README의 "⑤ 로컬 오토스케일" 절에서 인용하는 실제 실행 결과다. 재현 방법은 README ⑤ 절의 "스케일 아웃/인 확인 절차"를 따른다.

## 환경

- Windows 11, Docker Desktop(엔진 29.9.2, CPU 12, 메모리 약 8GB), kind v0.33.0(노드 kindest/node:v1.37.0, 1개), metrics-server v0.9.1
- 앱 이미지 `streamapp:local`(Dockerfile, 177MB), 앱 파드 requests CPU 100m·limits 500m, HPA 목표 CPU 평균 50%, 최소 2·최대 5
- 부하: `scripts\load-test.ps1`(기본값: k6 1.8.3, 가상 사용자 40, 늘리기 30초 → 유지 240초 → 줄이기 10초, 요청마다 새 연결), 부하 뒤 240초 더 관찰
- 부하 동안 RTMP 시험 송출(ffmpeg → `rtmp://localhost:1935/<채널ID>`)로 라이브 하나를 켜 두었다

## 기능 확인(curl·Python 클라이언트, 같은 세션 쿠키)

| 시점 | 확인 | 결과 |
| --- | --- | --- |
| 기동 직후(파드 2) | 두 파드가 빈 DB로 동시에 시작 | 한 파드가 `DB 연결·마이그레이션 실패(1/8) … 다시 시도합니다: while executing migration 5`를 남기고 0.2초 뒤 정상 시작. 기본 사용자는 한 번만 생성 |
| 기동 직후 | Redis보다 앱이 먼저 뜸 | `Redis에 연결하지 못했습니다` 경고 뒤 앱은 Ready, 26초 뒤 `Redis 구독 시작(streamapp:chat:*)` |
| 기동 직후 | MediaMTX보다 앱이 먼저 뜸(`STREAMAPP_MULTI_INSTANCE=true`) | `송출 상태를 알 수 없어 송출 상태 맞추기를 건너뜁니다(라이브는 그대로 둡니다)` |
| 파드 2 | 회원가입 → 녹화방송 올리기 → 댓글 | 회원가입은 `…-d5wsp`, 올리기(201)·댓글(200)은 `…-nw5wf`가 받음(앞 파드에서 받은 쿠키로) |
| 파드 2 | `GET /api/current-user` 12번(새 연결) | 12/12 로그인 상태, d5wsp 7·nw5wf 5 |
| 파드 2 | RTMP 송출 | 1초 안에 `/api/live-channels`에 뜸, `http://localhost:8888/<채널ID>/index.m3u8` 200(CORS `http://localhost:18080`) |
| 파드 2 | 채팅 WebSocket 두 개를 서로 다른 파드에 연결(보내는 쪽 nw5wf, 보는 쪽 d5wsp) | 보는 쪽이 `{"Message":…}`를 받음(Redis 중계), 보내는 쪽은 한 번만 받음. 송출을 끄자 보는 쪽(다른 파드)에 `"LiveEnded"`, 라이브 목록에서 사라짐 |
| 부하 중(파드 5, 22:26:23) | current-user 12번 | 12/12 로그인 상태, 4944w 5·wgsmv 3·9vvmt 2·nw5wf 1·d5wsp 1 |
| 부하 중 | 녹화방송 상세·영상(Range)·댓글 | 200, 206, 댓글 그대로. 라이브도 그대로 |
| 부하 중 | 스케일 아웃으로 뜬 파드(`…-wgsmv`) 시작 로그 | `MediaMTX에서 송출 중인 경로 1개를 확인했습니다` → `라이브 중인 채널 1개로 시작합니다`(진행 중 라이브를 닫지 않음) |
| 스케일 인 뒤(파드 2, 22:33:09) | current-user 12번, 녹화방송·댓글, 라이브 목록 | 12/12 로그인 상태(nw5wf 4·d5wsp 8), 녹화방송·영상·댓글 그대로, 라이브 그대로 |

## k6 요약(부하 전체, 스케일 아웃·인 포함)

```text
http_req_failed................: 0.00%  0 out of 188229
http_reqs......................: 188229 672.277098/s
http_req_duration..............: avg=1.85ms med=1.28ms max=92.13ms p(90)=3.64ms p(95)=4.75ms
vus_max........................: 40
✓ 'rate<0.02' rate=0.00%
```

## 인스턴스 수 변화(`scripts\k8s-watch.ps1` 출력 그대로)

열: 시각 | HPA가 본 평균 CPU(현재/목표) | HPA가 정한 파드 수 | 실제 앱 파드(Ready/전체) | 파드별 CPU | `/healthz` 12번 요청에 답한 파드(이름 끝 5글자=횟수, 응답 헤더 `x-streamapp-instance`)

```text
22:23:52 | 1%/50%            | 3        | 2/2                 | d5wsp=1m dh4s4=1m nw5wf=1m | d5wsp=5 nw5wf=7
22:24:04 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=3 nw5wf=9
22:24:15 | 70%/50%           | 2        | 3/3                 | d5wsp=79m nw5wf=61m | 9vvmt=2 d5wsp=7 nw5wf=3
22:24:25 | 70%/50%           | 3        | 3/3                 | 9vvmt=261m d5wsp=79m nw5wf=61m | 9vvmt=4 d5wsp=3 nw5wf=5
22:24:36 | 70%/50%           | 3        | 3/3                 | 9vvmt=261m d5wsp=79m nw5wf=61m | 9vvmt=1 d5wsp=5 nw5wf=6
22:24:47 | 318%/50%          | 3        | 5/5                 | 9vvmt=343m d5wsp=302m nw5wf=311m | 4944w=2 9vvmt=3 d5wsp=3 nw5wf=2 wgsmv=2
22:24:57 | 320%/50%          | 5        | 5/5                 | 4944w=177m 9vvmt=293m d5wsp=344m nw5wf=325m wgsmv=175m | 4944w=3 9vvmt=2 d5wsp=2 nw5wf=2 wgsmv=3
22:25:08 | 320%/50%          | 5        | 5/5                 | 4944w=202m 9vvmt=196m d5wsp=232m nw5wf=198m wgsmv=201m | 4944w=5 9vvmt=1 d5wsp=3 nw5wf=1 wgsmv=2
22:25:18 | 205%/50%          | 5        | 5/5                 | 4944w=202m 9vvmt=196m d5wsp=232m nw5wf=198m wgsmv=201m | 4944w=1 9vvmt=4 d5wsp=2 nw5wf=3 wgsmv=2
22:25:30 | 222%/50%          | 5        | 5/5                 | 4944w=216m 9vvmt=237m d5wsp=202m nw5wf=220m wgsmv=235m | 4944w=4 9vvmt=2 d5wsp=3 nw5wf=1 wgsmv=2
22:25:40 | 234%/50%          | 5        | 5/5                 | 4944w=240m 9vvmt=230m d5wsp=229m nw5wf=235m wgsmv=239m | 4944w=2 9vvmt=3 d5wsp=5 nw5wf=2
22:25:51 | 234%/50%          | 5        | 5/5                 | 4944w=240m 9vvmt=230m d5wsp=229m nw5wf=235m wgsmv=239m | 4944w=2 9vvmt=2 d5wsp=4 nw5wf=3 wgsmv=1
22:26:01 | 222%/50%          | 5        | 5/5                 | 4944w=224m 9vvmt=220m d5wsp=229m nw5wf=214m wgsmv=225m | 4944w=2 9vvmt=4 d5wsp=3 wgsmv=3
22:26:14 | 237%/50%          | 5        | 5/5                 | 4944w=233m 9vvmt=237m d5wsp=232m nw5wf=242m wgsmv=244m | 9vvmt=4 d5wsp=4 nw5wf=2 wgsmv=2
22:26:24 | 237%/50%          | 5        | 5/5                 | 4944w=229m 9vvmt=231m d5wsp=234m nw5wf=215m wgsmv=222m | 4944w=2 9vvmt=1 d5wsp=2 nw5wf=1 wgsmv=6
22:26:35 | 226%/50%          | 5        | 5/5                 | 4944w=229m 9vvmt=231m d5wsp=234m nw5wf=215m wgsmv=222m | 4944w=3 9vvmt=3 d5wsp=3 nw5wf=3
22:26:46 | 210%/50%          | 5        | 5/5                 | 4944w=209m 9vvmt=206m d5wsp=218m nw5wf=206m wgsmv=214m | 9vvmt=1 d5wsp=5 nw5wf=3 wgsmv=3
22:26:56 | 220%/50%          | 5        | 5/5                 | 4944w=212m 9vvmt=218m d5wsp=218m nw5wf=232m wgsmv=220m | 4944w=3 d5wsp=2 nw5wf=3 wgsmv=4
22:27:07 | 220%/50%          | 5        | 5/5                 | 4944w=212m 9vvmt=218m d5wsp=218m nw5wf=232m wgsmv=220m | 4944w=1 9vvmt=1 d5wsp=6 nw5wf=3 wgsmv=1
22:27:17 | 236%/50%          | 5        | 5/5                 | 4944w=234m 9vvmt=229m d5wsp=245m nw5wf=239m wgsmv=233m | 4944w=1 9vvmt=1 d5wsp=4 nw5wf=4 wgsmv=2
22:27:28 | 227%/50%          | 5        | 5/5                 | 4944w=223m 9vvmt=229m d5wsp=213m nw5wf=234m wgsmv=240m | 4944w=5 9vvmt=2 d5wsp=1 nw5wf=1 wgsmv=3
22:27:38 | 227%/50%          | 5        | 5/5                 | 4944w=233m 9vvmt=243m d5wsp=259m nw5wf=248m wgsmv=251m | 9vvmt=2 d5wsp=5 nw5wf=4 wgsmv=1
22:27:49 | 246%/50%          | 5        | 5/5                 | 4944w=233m 9vvmt=243m d5wsp=259m nw5wf=248m wgsmv=251m | 4944w=3 9vvmt=2 d5wsp=2 nw5wf=3 wgsmv=2
22:27:59 | 228%/50%          | 5        | 5/5                 | 4944w=236m 9vvmt=221m d5wsp=226m nw5wf=233m wgsmv=228m | 9vvmt=4 d5wsp=5 nw5wf=3
22:28:10 | 228%/50%          | 5        | 5/5                 | 4944w=236m 9vvmt=234m d5wsp=244m nw5wf=243m wgsmv=236m | 4944w=1 9vvmt=3 d5wsp=1 nw5wf=3 wgsmv=4
22:28:22 | 238%/50%          | 5        | 5/5                 | 4944w=236m 9vvmt=234m d5wsp=244m nw5wf=243m wgsmv=236m | 4944w=7 9vvmt=2 nw5wf=2 wgsmv=1
22:28:33 | 237%/50%          | 5        | 5/5                 | 4944w=236m 9vvmt=238m d5wsp=234m nw5wf=243m wgsmv=238m | 4944w=2 9vvmt=2 d5wsp=3 nw5wf=3 wgsmv=2
22:28:43 | 194%/50%          | 5        | 5/5                 | 4944w=204m 9vvmt=170m d5wsp=181m nw5wf=236m wgsmv=183m | 4944w=3 9vvmt=1 d5wsp=5 nw5wf=1 wgsmv=2
22:28:54 | 194%/50%          | 5        | 5/5                 | 4944w=1m 9vvmt=1m d5wsp=4m nw5wf=87m wgsmv=5m | 4944w=2 9vvmt=1 d5wsp=5 nw5wf=2 wgsmv=2
22:29:04 | 19%/50%           | 5        | 5/5                 | 4944w=1m 9vvmt=1m d5wsp=4m nw5wf=87m wgsmv=5m | 4944w=4 9vvmt=4 d5wsp=2 nw5wf=1 wgsmv=1
22:29:15 | 1%/50%            | 5        | 5/5                 | 4944w=1m 9vvmt=1m d5wsp=4m nw5wf=1m wgsmv=1m | 4944w=4 9vvmt=2 nw5wf=3 wgsmv=3
22:29:25 | 1%/50%            | 5        | 5/5                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m wgsmv=1m | 4944w=4 9vvmt=1 d5wsp=2 nw5wf=1 wgsmv=4
22:29:36 | 1%/50%            | 5        | 5/5                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m wgsmv=1m | 4944w=1 9vvmt=2 d5wsp=3 nw5wf=4 wgsmv=2
22:29:46 | 1%/50%            | 5        | 4/4                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m wgsmv=1m | 4944w=1 9vvmt=3 d5wsp=5 nw5wf=3
22:29:57 | 1%/50%            | 4        | 3/3                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m wgsmv=1m | 9vvmt=5 d5wsp=6 nw5wf=1
22:30:07 | 1%/50%            | 4        | 3/3                 | 4944w=1m 9vvmt=1m d5wsp=1m nw5wf=1m | 9vvmt=3 d5wsp=4 nw5wf=5
22:30:17 | 1%/50%            | 3        | 2/2                 | 9vvmt=2m d5wsp=2m nw5wf=1m | d5wsp=6 nw5wf=6
22:30:30 | 1%/50%            | 2        | 2/2                 | 9vvmt=1m d5wsp=1m nw5wf=2m | d5wsp=6 nw5wf=6
22:30:40 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=7 nw5wf=5
22:30:51 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=8 nw5wf=4
22:31:01 | 1%/50%            | 2        | 2/2                 | d5wsp=2m nw5wf=1m | d5wsp=7 nw5wf=5
22:31:12 | 1%/50%            | 2        | 2/2                 | d5wsp=2m nw5wf=1m | d5wsp=4 nw5wf=8
22:31:22 | 1%/50%            | 2        | 2/2                 | d5wsp=2m nw5wf=1m | d5wsp=5 nw5wf=7
22:31:33 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=5 nw5wf=7
22:31:43 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=2m | d5wsp=6 nw5wf=6
22:31:54 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=6 nw5wf=6
22:32:04 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=4 nw5wf=8
22:32:14 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=1m | d5wsp=4 nw5wf=8
22:32:25 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=2m | d5wsp=5 nw5wf=7
22:32:37 | 1%/50%            | 2        | 2/2                 | d5wsp=1m nw5wf=2m | d5wsp=6 nw5wf=6
```

읽는 법:

- **22:24:04 → 22:24:15**: 부하가 붙자 CPU 70%/50% → HPA가 2 → 3개로 늘림.
- **22:24:47**: CPU 318%/50% → 3 → 5개(최대). 새 파드도 곧바로 요청을 받는다(분산 열에 `4944w`, `wgsmv`).
- **22:24:57 ~ 22:28:43**: 5개 유지, 파드별 CPU가 고르게 약 200~250m, 요청이 5개 파드에 나뉨.
- **22:28:54**: 부하 끝. CPU가 1%로 떨어진 뒤 60초 안정화 시간이 지나 **22:29:46**부터 15초마다 1개씩 5 → 4 → 3 → 2개로 줄어 **22:30:30**에 최소 2개.
- 줄어드는 동안에도 분산 확인 요청 실패(`실패=`)가 없었다.

## 처음 시도에서 고친 것

- k6를 `host.docker.internal:18080`(Docker Desktop의 호스트 포트 중계 → kind 포트 매핑)으로 보내자, 요청마다 새 연결을 맺는 부하에서 `dial: i/o timeout`이 쌓여 부하가 1분 만에 떨어졌다. k6 컨테이너를 kind의 Docker 네트워크에 붙여 노드의 NodePort(`streamapp-control-plane:30080`)로 바로 보내도록 바꾼 뒤 위 결과를 얻었다.
- 관찰 스크립트가 한 줄짜리 `kubectl get hpa` 출력을 문자열로 받아 HPA 열을 읽지 못했다(`@()`로 감싸 고침).
- Dockerfile의 빌드·실행 이미지를 bookworm에서 trixie로 바꿨다: dx 0.7.10의 미리 빌드된 리눅스 파일이 glibc 2.39를 요구한다.

## 정리

확인을 마친 뒤 `scripts\k8s-down.ps1`로 kind 클러스터를 지웠다(공유 볼륨의 DB·영상도 함께 사라진다).

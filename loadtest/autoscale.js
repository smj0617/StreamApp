// 로컬 오토스케일(과제 ⑤) 부하 스크립트(k6). scripts\load-test.ps1이 Docker의 grafana/k6 이미지로 돌린다:
//   docker run --rm -i --network kind -v <레포>\loadtest:/scripts grafana/k6:1.8.3 run /scripts/autoscale.js
//
// 대상은 kind 노드의 앱 Service NodePort(http://streamapp-control-plane:30080)다. k6 컨테이너를 kind의 Docker
// 네트워크에 붙여 바로 보낸다(호스트 포트 18080을 거치면 Docker Desktop 포트 중계가 새 연결 폭주를 감당하지 못한다).
// 단계: 늘리기(RAMP) → 유지(HOLD) → 줄이기(10초) → 멈춤. 멈춘 뒤 HPA가 파드를 다시 줄이는지 본다.
//
// 요청은 CPU를 쓰는 것 위주로 섞는다(가중치):
//   4  GET /                   홈 화면 SSR(라이브·녹화방송 목록을 읽어 HTML을 그린다)
//   2  GET /api/recordings     녹화방송 목록(DB)
//   2  GET /api/live-channels  라이브 목록(DB)
//   1  GET /healthz            상태 확인(DB SELECT 1 + MediaMTX 확인)
//
// noConnectionReuse: 요청마다 새 연결을 맺는다. Service(kube-proxy)는 연결 단위로 파드를 고르므로, 연결을 재사용하면
// 부하 중에 새로 뜬 파드가 요청을 받지 못한다.
//
// 환경변수: BASE_URL(기본 http://streamapp-control-plane:30080), VUS(기본 40), RAMP(기본 30s), HOLD(기본 4m)
import http from 'k6/http';
import { check, sleep } from 'k6';

const BASE_URL = __ENV.BASE_URL || 'http://streamapp-control-plane:30080';
const VUS = parseInt(__ENV.VUS || '40', 10);

export const options = {
  noConnectionReuse: true,
  scenarios: {
    autoscale: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: __ENV.RAMP || '30s', target: VUS },
        { duration: __ENV.HOLD || '4m', target: VUS },
        { duration: '10s', target: 0 },
      ],
      gracefulRampDown: '5s',
    },
  },
  thresholds: {
    // 스케일 아웃·인 중에도 요청이 거의 실패하지 않아야 한다.
    http_req_failed: ['rate<0.02'],
  },
};

const REQUESTS = [
  ['/', 4],
  ['/api/recordings', 2],
  ['/api/live-channels', 2],
  ['/healthz', 1],
];
const TOTAL_WEIGHT = REQUESTS.reduce((sum, [, weight]) => sum + weight, 0);

function pickPath() {
  let roll = Math.random() * TOTAL_WEIGHT;
  for (const [path, weight] of REQUESTS) {
    roll -= weight;
    if (roll < 0) return path;
  }
  return REQUESTS[0][0];
}

export default function () {
  const path = pickPath();
  const res = http.get(`${BASE_URL}${path}`, { tags: { name: path } });
  check(res, { '200': (r) => r.status === 200 });
  sleep(0.05);
}

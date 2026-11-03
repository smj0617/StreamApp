use dioxus::prelude::*;

/// hls.js. 버전을 고정한다(MediaMTX v1.21.3의 내장 플레이어는 1.7.3을 쓴다).
const HLS_JS: &str = "https://cdn.jsdelivr.net/npm/hls.js@1.7.4/dist/hls.min.js";

/// 플레이어 `<video>`의 id. 한 페이지에 플레이어는 하나다.
const VIDEO_ID: &str = "live-video";

/// `<video>`에 hls.js를 붙여 재생 주소를 재생한다. 재생 주소는 `dioxus.send`로 받는다
/// (JS 문자열에 직접 넣지 않으므로 이스케이프할 필요가 없다).
const ATTACH_JS: &str = r#"
const src = await dioxus.recv();
const video = document.getElementById("live-video");
if (!video) return;
// <script>가 아직 다 받아지지 않았을 수 있으므로 window.Hls를 기다린다(최대 15초).
for (let waited = 0; !window.Hls; waited += 100) {
  if (waited >= 15000) { console.error("hls.js를 불러오지 못했습니다"); return; }
  await new Promise((resolve) => setTimeout(resolve, 100));
}
if (window.__streamappHls) { window.__streamappHls.destroy(); window.__streamappHls = null; }
// 브라우저는 소리가 꺼진 영상만 자동 재생한다.
video.muted = true;
if (Hls.isSupported()) {
  const hls = new Hls({ lowLatencyMode: true });
  window.__streamappHls = hls;
  hls.on(Hls.Events.MEDIA_ATTACHED, () => hls.loadSource(src));
  hls.on(Hls.Events.MANIFEST_PARSED, () => video.play().catch(() => {}));
  hls.on(Hls.Events.ERROR, (_event, data) => {
    if (!data.fatal) return;
    if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
      hls.recoverMediaError();
    } else if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
      // hls.js가 재시도를 다 쓴 뒤다. 바로 다시 받으면 요청이 몰리므로 잠시 쉬고 처음부터 다시 받는다.
      setTimeout(() => { if (window.__streamappHls === hls) hls.loadSource(src); }, 3000);
    } else {
      hls.destroy();
      window.__streamappHls = null;
    }
  });
  // 일시정지했다가 다시 재생하면 라이브 지점으로 간다(MediaMTX 문서 권장).
  video.onplay = () => { if (hls.liveSyncPosition) video.currentTime = hls.liveSyncPosition; };
  hls.attachMedia(video);
} else if (video.canPlayType("application/vnd.apple.mpegurl")) {
  // Safari: 브라우저가 HLS를 직접 재생한다.
  video.src = src;
  video.play().catch(() => {});
}
"#;

const DESTROY_JS: &str =
    "if (window.__streamappHls) { window.__streamappHls.destroy(); window.__streamappHls = null; }";

/// 라이브 영상 플레이어: hls.js로 HLS 재생 주소를 소리 끈 채 자동 재생한다.
///
/// 붙이기는 브라우저에서 화면에 그린 뒤에만 한다(서버 렌더링에서는 `use_effect`가 돌지 않는다).
/// 재생 주소가 바뀌면 다시 붙도록 쓰는 쪽에서 `key`를 재생 주소로 준다.
#[component]
pub fn HlsPlayer(src: String) -> Element {
    // 컴포넌트가 사라질 때는 컨텍스트를 찾지 못할 수 있으므로 document 핸들을 미리 잡아 둔다.
    let doc = use_hook(document::document);
    let src_for_attach = src.clone();

    use_effect(move || {
        let attach = document::eval(ATTACH_JS);
        let _ = attach.send(src_for_attach.clone());
    });

    use_drop(move || {
        let _ = doc.eval(DESTROY_JS.to_string());
    });

    rsx! {
        document::Script { src: HLS_JS, defer: true }
        video {
            id: VIDEO_ID,
            class: "watch-player",
            controls: true,
            autoplay: true,
            muted: true,
            playsinline: true,
        }
    }
}

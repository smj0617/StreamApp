//! 영상 플레이어: 라이브(HLS)와 녹화방송(mp4). 웹뷰의 `<video>`로 재생한다.
//!
//! - 라이브: macOS(WKWebView)는 HLS를 직접 재생하므로 원래 재생 주소를 `<video src>`에 넣는다. Windows(WebView2)는
//!   HLS를 직접 재생하지 못하므로 앱에 넣어 둔 hls.js가 앱의 HLS 중계([`crate::hls_proxy`]) 주소로 받는다.
//! - 녹화방송: 서버의 mp4 주소를 `<video src>`에 넣는다(Range 요청은 웹뷰가 서버에 직접 한다).

use dioxus::prelude::*;
use serde::Serialize;

use crate::hls_proxy;

/// hls.js 1.7.4(Apache-2.0). 웹 앱(`packages/web/src/views/hls_player.rs`)과 같은 버전을 CDN 대신 앱에 넣어 둔다.
const HLS_JS: Asset = asset!("/assets/hls.min.js");

/// 라이브 플레이어 `<video>`의 id. 화면에 라이브 플레이어는 하나다.
const LIVE_VIDEO_ID: &str = "live-video";

/// hls.js가 준비되면 재생을 붙인다. 재생 주소들은 `dioxus.send`로 받는다(JS 문자열에 넣지 않는다).
const ATTACH_JS: &str = r#"
const { videoId, direct, proxied, preferNative } = await dioxus.recv();
const video = document.getElementById(videoId);
if (!video) return;
if (window.__streamappHls) { window.__streamappHls.destroy(); window.__streamappHls = null; }
video.muted = true;
const nativeHls = video.canPlayType("application/vnd.apple.mpegurl") !== "";
if (preferNative && nativeHls) {
  video.src = direct;
  video.play().catch(() => {});
  return;
}
// <script>가 아직 다 읽히지 않았을 수 있으므로 window.Hls를 기다린다(최대 15초).
for (let waited = 0; !window.Hls; waited += 100) {
  if (waited >= 15000) break;
  await new Promise((resolve) => setTimeout(resolve, 100));
}
if (window.Hls && Hls.isSupported() && proxied) {
  const hls = new Hls({ lowLatencyMode: true });
  window.__streamappHls = hls;
  hls.on(Hls.Events.MEDIA_ATTACHED, () => hls.loadSource(proxied));
  hls.on(Hls.Events.MANIFEST_PARSED, () => video.play().catch(() => {}));
  hls.on(Hls.Events.ERROR, (_event, data) => {
    if (!data.fatal) return;
    if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
      hls.recoverMediaError();
    } else if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
      // 재시도를 다 쓴 뒤다. 잠시 쉬고 처음부터 다시 받는다(송출이 잠깐 끊긴 경우).
      setTimeout(() => { if (window.__streamappHls === hls) hls.loadSource(proxied); }, 3000);
    } else {
      hls.destroy();
      window.__streamappHls = null;
    }
  });
  video.onplay = () => { if (hls.liveSyncPosition) video.currentTime = hls.liveSyncPosition; };
  hls.attachMedia(video);
} else if (nativeHls) {
  video.src = direct;
  video.play().catch(() => {});
} else {
  console.error("이 웹뷰는 HLS를 재생할 수 없습니다");
}
"#;

const DESTROY_JS: &str =
    "if (window.__streamappHls) { window.__streamappHls.destroy(); window.__streamappHls = null; }";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachRequest {
    video_id: &'static str,
    /// 원래 재생 주소(네이티브 HLS용)
    direct: String,
    /// 웹뷰 안 중계 주소(hls.js용). 재생 주소가 http(s)가 아니면 없다
    proxied: Option<String>,
    /// 네이티브 HLS를 먼저 쓸지. macOS만 그렇다(WebView2는 hls.js가 더 안정적이다)
    prefer_native: bool,
}

/// 라이브 플레이어. `src`는 전체 HLS 재생 주소다. 주소가 바뀌면 다시 붙도록 쓰는 쪽에서 `key`를 준다.
#[component]
pub fn LivePlayer(src: String) -> Element {
    let doc = use_hook(document::document);

    use_effect(move || {
        let attach = document::eval(ATTACH_JS);
        let _ = attach.send(AttachRequest {
            video_id: LIVE_VIDEO_ID,
            direct: src.clone(),
            proxied: hls_proxy::proxied_path(&src),
            prefer_native: cfg!(target_os = "macos"),
        });
    });

    use_drop(move || {
        let _ = doc.eval(DESTROY_JS.to_string());
    });

    rsx! {
        document::Script { src: HLS_JS }
        video {
            id: LIVE_VIDEO_ID,
            class: "player",
            controls: true,
            autoplay: true,
            muted: true,
            playsinline: true,
        }
    }
}

/// 녹화방송 플레이어. `src`는 전체 mp4 주소다.
#[component]
pub fn RecordingPlayer(src: String) -> Element {
    rsx! {
        video {
            class: "player",
            src: "{src}",
            controls: true,
            playsinline: true,
            preload: "metadata",
        }
    }
}

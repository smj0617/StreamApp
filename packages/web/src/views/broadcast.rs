use api::dto::MyChannelSettings;
use api::rules::{fallback_title, MAX_TITLE_CHARS};
use api::BROADCAST_WHIP_PATH;
use dioxus::prelude::*;
use serde::Deserialize;

use super::script_with_element_id;
use super::shell::{rejection_reason, CurrentUser, LoginRequired};
use crate::Route;

/// 카메라 미리보기 `<video>`의 id.
const PREVIEW_ID: &str = "broadcast-preview";

/// 카메라·마이크를 켜고 미리보기에 붙인다. 받는 값: 카메라 방향(`"user"` 전면, `"environment"` 후면).
/// 돌려주는 값([`CameraResult`]): `{ok: true}` 또는 `{error: <DOMException 이름>}`.
/// 브라우저 상태는 `window.__streamappBroadcast`(stream, pc, session)에 둔다. `elementId`는 [`PREVIEW_ID`]다.
const CAMERA_JS: &str = r#"
const facing = await dioxus.recv();
const state = window.__streamappBroadcast || (window.__streamappBroadcast = {});
// 휴대폰 브라우저는 HTTPS(또는 localhost)에서만 카메라를 연다.
if (!window.isSecureContext || !navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
  return { error: "InsecureContext" };
}
if (state.pc) return { error: "Busy" };
if (state.stream) state.stream.getTracks().forEach((track) => track.stop());
state.stream = null;
try {
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { facingMode: facing, width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } },
    audio: { echoCancellation: true, noiseSuppression: true },
  });
  state.stream = stream;
  const video = document.getElementById(elementId);
  if (video) {
    video.srcObject = stream;
    video.muted = true;
    video.play().catch(() => {});
  }
  return { ok: true };
} catch (e) {
  return { error: (e && e.name) || "Error" };
}
"#;

/// 켜 둔 카메라·마이크를 WebRTC(WHIP)로 송출한다. 받는 값: WHIP 주소([`BROADCAST_WHIP_PATH`]).
/// 보내는 값([`PublishEvent`]): `connecting` → `live`, 도중에 `unstable`, 끝나면 `stopped` 또는 `failed`.
/// 끝날 때까지(종료 버튼, 연결 실패) 스크립트가 살아 있다.
///
/// 영상은 H.264로만 보낸다. MediaMTX가 시청용 HLS로 바꿀 수 있는 코덱이기 때문이다(VP8은 못 바꾼다).
/// 소리는 Opus다.
const PUBLISH_JS: &str = r#"
const url = await dioxus.recv();
const state = window.__streamappBroadcast || (window.__streamappBroadcast = {});
let finished = false;
let release;
const done = new Promise((resolve) => { release = resolve; });
const finish = (event) => {
  if (finished) return;
  finished = true;
  dioxus.send(event);
  release();
};
if (!state.stream) { finish({ kind: "failed", message: "카메라를 먼저 켜 주세요" }); return; }
if (state.pc) { finish({ kind: "failed", message: "이미 송출 중입니다" }); return; }
const caps = window.RTCRtpSender && RTCRtpSender.getCapabilities ? RTCRtpSender.getCapabilities("video") : null;
const h264 = caps ? caps.codecs.filter((codec) => codec.mimeType.toLowerCase() === "video/h264") : [];
if (h264.length === 0) { finish({ kind: "no_h264" }); return; }

const pc = new RTCPeerConnection();
state.pc = pc;
state.session = null;
window.addEventListener("streamapp-broadcast-stopped", () => finish({ kind: "stopped" }), { once: true });
const fail = (message) => {
  if (state.pc === pc) { state.pc = null; state.session = null; }
  pc.close();
  finish({ kind: "failed", message });
};
for (const track of state.stream.getTracks()) {
  const transceiver = pc.addTransceiver(track, { direction: "sendonly", streams: [state.stream] });
  if (track.kind === "video" && transceiver.setCodecPreferences) transceiver.setCodecPreferences(h264);
}
pc.onconnectionstatechange = () => {
  if (finished) return;
  if (pc.connectionState === "connected") dioxus.send({ kind: "live" });
  else if (pc.connectionState === "disconnected") dioxus.send({ kind: "unstable" });
  else if (pc.connectionState === "failed") {
    fail("PC와 영상 연결을 맺지 못했거나 끊겼습니다. 휴대폰과 PC가 같은 와이파이인지, PC 방화벽이 mediamtx.exe(8189 포트)를 허용하는지 확인해 주세요");
  }
};
try {
  await pc.setLocalDescription(await pc.createOffer());
  // ICE 후보를 다 모은 뒤 한 번에 보낸다(최대 3초). 서버의 후보는 응답에 들어 있다.
  await new Promise((resolve) => {
    if (pc.iceGatheringState === "complete") return resolve();
    const timer = setTimeout(resolve, 3000);
    pc.addEventListener("icegatheringstatechange", () => {
      if (pc.iceGatheringState === "complete") { clearTimeout(timer); resolve(); }
    });
  });
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/sdp" },
    body: pc.localDescription.sdp,
  });
  if (res.status !== 201) {
    let message = null;
    try { message = (await res.json()).message; } catch (e) {}
    fail(message || `송출을 시작하지 못했습니다(HTTP ${res.status})`);
  } else {
    state.session = res.headers.get("Location");
    await pc.setRemoteDescription({ type: "answer", sdp: await res.text() });
    dioxus.send({ kind: "connecting" });
  }
} catch (e) {
  fail("송출을 시작하지 못했습니다: " + ((e && e.message) || e));
}
await done;
"#;

/// 송출을 끝낸다: 연결을 닫고, 앱을 거쳐 MediaMTX 세션을 지우고, 카메라·마이크를 끈다.
/// 송출 중이 아니어도 카메라를 끈다. `elementId`는 [`PREVIEW_ID`]다.
const STOP_JS: &str = r#"
const state = window.__streamappBroadcast || {};
const pc = state.pc;
const session = state.session;
state.pc = null;
state.session = null;
if (pc) pc.close();
if (state.stream) state.stream.getTracks().forEach((track) => track.stop());
state.stream = null;
const video = document.getElementById(elementId);
if (video) video.srcObject = null;
window.dispatchEvent(new Event("streamapp-broadcast-stopped"));
if (session) {
  try { await fetch(session, { method: "DELETE", keepalive: true }); } catch (e) {}
}
return true;
"#;

/// 켜 둔 카메라의 방향만 바꾼다. 송출 중이면 연결은 그대로 두고 보내는 영상 트랙만 바꾼다(`replaceTrack`).
/// 받는 값: 카메라 방향. 돌려주는 값([`CameraResult`]). 휴대폰은 카메라 두 개를 동시에 열지 못하는 경우가
/// 많아, 지금 영상 트랙을 먼저 끄고 새 방향으로 연다. `elementId`는 [`PREVIEW_ID`]다.
const SWITCH_JS: &str = r#"
const facing = await dioxus.recv();
const state = window.__streamappBroadcast || {};
if (!state.stream) return { error: "NoCamera" };
for (const track of state.stream.getVideoTracks()) {
  track.stop();
  state.stream.removeTrack(track);
}
let fresh;
try {
  fresh = await navigator.mediaDevices.getUserMedia({
    video: { facingMode: facing, width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } },
  });
} catch (e) {
  return { error: (e && e.name) || "Error" };
}
const track = fresh.getVideoTracks()[0];
state.stream.addTrack(track);
if (state.pc) {
  const sender = state.pc.getSenders().find((s) => s.track && s.track.kind === "video")
    || state.pc.getTransceivers().map((t) => t.sender).find((s) => !s.track || s.track.kind === "video");
  if (sender) await sender.replaceTrack(track);
}
const video = document.getElementById(elementId);
if (video) {
  video.srcObject = state.stream;
  video.play().catch(() => {});
}
return { ok: true };
"#;

/// [`CAMERA_JS`]·[`SWITCH_JS`]의 결과.
#[derive(Deserialize)]
struct CameraResult {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// [`PUBLISH_JS`]가 보내는 메시지.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublishEvent {
    /// 미디어 서버가 받아들여 영상 연결을 맺는 중
    Connecting,
    /// 영상이 가고 있다
    Live,
    /// 잠깐 끊겼다(브라우저가 다시 붙으려 한다)
    Unstable,
    /// 종료 버튼으로 끝냈다
    Stopped,
    /// 이 브라우저가 H.264로 보낼 수 없다
    NoH264,
    Failed {
        message: String,
    },
}

/// 휴대폰 방송 화면의 단계.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// 카메라 꺼짐
    Off,
    /// 카메라 켜는 중(권한 묻는 중)
    Opening,
    /// 미리보기 중, 송출 전
    Preview,
    /// 송출 요청을 보내고 영상 연결을 맺는 중
    Connecting,
    /// 송출 중
    Live,
    /// 송출 중 연결이 잠깐 끊김
    Unstable,
}

impl Phase {
    fn publishing(self) -> bool {
        matches!(self, Phase::Connecting | Phase::Live | Phase::Unstable)
    }
}

/// 휴대폰 방송 `/broadcast`: 휴대폰 브라우저에서 카메라·마이크로 내 채널에 라이브를 시작하고 끝낸다.
/// 로그인해야 쓸 수 있고, 송출 시작·종료 권한은 서버가 세션으로 확인한다(ADR 0005).
#[component]
pub fn Broadcast() -> Element {
    let CurrentUser(current) = use_context::<CurrentUser>();
    let user = current.read().clone();

    rsx! {
        section { class: "settings broadcast",
            h1 { "휴대폰 방송" }
            match user {
                None => rsx! {
                    LoginRequired { what: "내 채널로 방송할 수 있습니다." }
                },
                // 로그인한 사용자가 바뀌면 화면을 새로 시작한다(이전 사용자의 송출은 끝난다).
                Some(user) => rsx! {
                    SuspenseBoundary {
                        fallback: |_| rsx! {
                            p { class: "muted", "불러오는 중…" }
                        },
                        Studio { key: "{user.id}", nickname: user.nickname }
                    }
                },
            }
        }
    }
}

/// 현재 사용자 채널의 설정을 불러와 방송 화면을 그린다.
#[component]
fn Studio(nickname: String) -> Element {
    let loaded = use_server_future(api::my_channel_settings)?;
    let state = (*loaded.read()).clone();

    match state {
        Some(Ok(Some(settings))) => rsx! {
            Camera { settings, nickname }
        },
        Some(Ok(None)) => rsx! {
            LoginRequired { what: "내 채널로 방송할 수 있습니다." }
        },
        Some(Err(error)) => {
            let reason = rejection_reason(error);
            rsx! {
                p { class: "form-error", role: "alert", "채널 정보를 불러오지 못했습니다: {reason}" }
            }
        }
        None => rsx! {},
    }
}

/// 카메라 미리보기, 라이브 제목, 카메라 방향, 시작·종료 버튼, 상태.
#[component]
fn Camera(settings: MyChannelSettings, nickname: String) -> Element {
    let mut phase = use_signal(|| Phase::Off);
    // `"user"` 전면, `"environment"` 후면
    let mut facing = use_signal(|| "user");
    let mut title = use_signal(|| settings.next_live_title.clone().unwrap_or_default());
    let mut error = use_signal(|| None::<String>);
    let doc = use_hook(document::document);

    // 페이지를 떠나면 송출과 카메라를 끝낸다.
    use_drop(move || {
        let _ = doc.eval(script_with_element_id(PREVIEW_ID, STOP_JS));
    });

    let open = move || async move {
        error.set(None);
        phase.set(Phase::Opening);
        let eval = document::eval(&script_with_element_id(PREVIEW_ID, CAMERA_JS));
        let result = match eval.send(facing()) {
            Ok(()) => eval.join::<CameraResult>().await.ok(),
            Err(_) => None,
        };
        match result {
            Some(CameraResult { ok: true, .. }) => phase.set(Phase::Preview),
            Some(CameraResult { error: name, .. }) => {
                phase.set(Phase::Off);
                error.set(Some(error_reason(name.as_deref())));
            }
            None => {
                phase.set(Phase::Off);
                error.set(Some("카메라를 켜지 못했습니다".to_string()));
            }
        }
    };

    // 카메라 방향 바꾸기: 카메라가 꺼져 있으면 다음에 켤 방향만 정하고, 켜져 있으면(송출 중 포함) 바로 바꾼다.
    let mut switching = use_signal(|| false);
    let switch_to = move |to: &'static str| async move {
        if facing() == to || switching() {
            return;
        }
        let before = facing();
        facing.set(to);
        if phase() == Phase::Off {
            return;
        }
        error.set(None);
        switching.set(true);
        let eval = document::eval(&script_with_element_id(PREVIEW_ID, SWITCH_JS));
        let result = match eval.send(to) {
            Ok(()) => eval.join::<CameraResult>().await.ok(),
            Err(_) => None,
        };
        switching.set(false);
        match result {
            Some(CameraResult { ok: true, .. }) => {}
            Some(CameraResult { error: name, .. }) => {
                facing.set(before);
                error.set(Some(error_reason(name.as_deref())));
            }
            None => {
                facing.set(before);
                error.set(Some("카메라를 바꾸지 못했습니다".to_string()));
            }
        }
    };

    let start = move || async move {
        if phase() != Phase::Preview {
            return;
        }
        error.set(None);
        phase.set(Phase::Connecting);
        // 라이브 제목은 송출이 시작될 때의 다음 라이브 제목이다. 먼저 저장한다.
        match api::change_next_live_title(title()).await {
            Ok(saved) => title.set(saved.unwrap_or_default()),
            Err(e) => {
                phase.set(Phase::Preview);
                error.set(Some(rejection_reason(e)));
                return;
            }
        }
        let mut eval = document::eval(&script_with_element_id(PREVIEW_ID, PUBLISH_JS));
        if eval.send(BROADCAST_WHIP_PATH).is_err() {
            phase.set(Phase::Preview);
            error.set(Some("송출을 시작하지 못했습니다".to_string()));
            return;
        }
        loop {
            match eval.recv::<PublishEvent>().await {
                Ok(PublishEvent::Connecting) => phase.set(Phase::Connecting),
                Ok(PublishEvent::Live) => phase.set(Phase::Live),
                Ok(PublishEvent::Unstable) => phase.set(Phase::Unstable),
                Ok(PublishEvent::Stopped) => {
                    phase.set(Phase::Off);
                    break;
                }
                Ok(PublishEvent::NoH264) => {
                    phase.set(Phase::Preview);
                    error.set(Some(
                        "이 브라우저는 H.264 영상으로 보낼 수 없어 방송할 수 없습니다. 휴대폰의 Chrome이나 Safari 최신 버전을 써 주세요".to_string(),
                    ));
                    break;
                }
                Ok(PublishEvent::Failed { message }) => {
                    phase.set(Phase::Preview);
                    error.set(Some(message));
                    break;
                }
                Err(_) => break,
            }
        }
    };

    let stop = move || async move {
        let eval = document::eval(&script_with_element_id(PREVIEW_ID, STOP_JS));
        let _ = eval.join::<bool>().await;
        phase.set(Phase::Off);
    };

    let current = phase();
    let length = title.read().trim().chars().count();
    let count_class = if length > MAX_TITLE_CHARS {
        "title-count over"
    } else {
        "title-count"
    };
    let placeholder = fallback_title(&nickname);
    let mirrored = facing() == "user";
    let (status_class, status_text) = match current {
        Phase::Off => ("broadcast-status", "카메라 꺼짐"),
        Phase::Opening => (
            "broadcast-status",
            "카메라를 켜는 중… 권한을 물으면 허용해 주세요",
        ),
        Phase::Preview => ("broadcast-status", "미리보기 중 · 아직 방송 전입니다"),
        Phase::Connecting => ("broadcast-status pending", "연결 중…"),
        Phase::Live => ("broadcast-status live", "방송 중"),
        Phase::Unstable => (
            "broadcast-status pending",
            "연결이 불안정합니다 · 다시 붙는 중…",
        ),
    };

    rsx! {
        div { class: "watch-stage",
            video {
                id: PREVIEW_ID,
                class: if mirrored { "watch-player broadcast-preview mirrored" } else { "watch-player broadcast-preview" },
                autoplay: true,
                muted: true,
                playsinline: true,
            }
            if current == Phase::Live {
                span { class: "live-badge broadcast-live-badge", "LIVE" }
            }
        }
        p { class: "{status_class}", role: "status", "{status_text}" }
        if let Some(reason) = error() {
            p { class: "form-error", role: "alert", "{reason}" }
        }
        div { class: "form-actions broadcast-actions",
            match current {
                Phase::Off | Phase::Opening => rsx! {
                    button {
                        class: "primary-button",
                        disabled: current == Phase::Opening,
                        onclick: move |_| async move { open().await },
                        "카메라 켜기"
                    }
                },
                Phase::Preview => rsx! {
                    button {
                        class: "primary-button live",
                        onclick: move |_| async move { start().await },
                        "방송 시작"
                    }
                    button { onclick: move |_| async move { stop().await }, "카메라 끄기" }
                },
                _ => rsx! {
                    button {
                        class: "primary-button stop",
                        onclick: move |_| async move { stop().await },
                        "방송 종료"
                    }
                },
            }
            Link {
                class: "heading-action",
                to: Route::Watch {
                    channel_id: settings.channel_id.clone(),
                },
                "내 시청 페이지"
            }
        }
        section { class: "settings-card",
            div { class: "field",
                label { class: "field-label", r#for: "broadcast-title", "라이브 제목" }
                div { class: "field-row",
                    input {
                        id: "broadcast-title",
                        placeholder,
                        value: "{title}",
                        disabled: current.publishing(),
                        oninput: move |event| title.set(event.value()),
                    }
                }
                p { class: "{count_class}", "{length}/{MAX_TITLE_CHARS}" }
                p { class: "muted small",
                    "방송을 시작할 때 이 제목이 라이브 제목이 됩니다(내 채널 설정의 다음 라이브 제목과 같습니다). 방송 중 제목은 내 채널 설정에서 바꿀 수 있습니다."
                }
            }
            div { class: "field",
                span { class: "field-label", "카메라" }
                div { class: "field-row",
                    button {
                        class: if facing() == "user" { "choice selected" } else { "choice" },
                        aria_pressed: "{facing() == \"user\"}",
                        disabled: current == Phase::Opening || switching(),
                        onclick: move |_| async move { switch_to("user").await },
                        "전면"
                    }
                    button {
                        class: if facing() == "environment" { "choice selected" } else { "choice" },
                        aria_pressed: "{facing() == \"environment\"}",
                        disabled: current == Phase::Opening || switching(),
                        onclick: move |_| async move { switch_to("environment").await },
                        "후면"
                    }
                }
            }
            p { class: "muted small",
                "휴대폰과 PC가 같은 와이파이에 있어야 합니다. 휴대폰 브라우저는 HTTPS 주소에서만 카메라를 열어 주므로, PC에서 터널(README의 휴대폰 방송 절)을 띄운 주소로 이 페이지를 여세요. 방송 중에는 화면을 끄거나 다른 앱으로 가지 마세요."
            }
        }
    }
}

/// 카메라를 켜지 못한 이유(DOMException 이름)를 화면에 보여 줄 문장으로 바꾼다.
fn error_reason(name: Option<&str>) -> String {
    match name {
        Some("InsecureContext") => "이 주소에서는 브라우저가 카메라를 열어 주지 않습니다. HTTPS 주소(README의 터널)나 PC의 localhost로 열어 주세요".to_string(),
        Some("NotAllowedError") | Some("SecurityError") => "카메라·마이크 권한이 거부되었습니다. 브라우저 주소창의 사이트 설정에서 카메라와 마이크를 허용한 뒤 다시 눌러 주세요".to_string(),
        Some("NotFoundError") | Some("OverconstrainedError") => "쓸 수 있는 카메라나 마이크를 찾지 못했습니다".to_string(),
        Some("NotReadableError") | Some("AbortError") => "다른 앱이 카메라나 마이크를 쓰고 있습니다. 그 앱을 닫고 다시 눌러 주세요".to_string(),
        Some("Busy") => "방송 중에는 카메라를 다시 켤 수 없습니다. 방송을 끝낸 뒤 눌러 주세요".to_string(),
        Some("NoCamera") => "카메라를 먼저 켜 주세요".to_string(),
        Some(other) => format!("카메라를 켜지 못했습니다({other})"),
        None => "카메라를 켜지 못했습니다".to_string(),
    }
}

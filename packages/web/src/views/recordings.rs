use api::rules::{MAX_RECORDING_BYTES, MAX_RECORDING_MB, MAX_TITLE_CHARS, RECORDING_EXTENSIONS};
use api::{RECORDING_UPLOAD_PATH, UPLOAD_FILE_FIELD, UPLOAD_TITLE_FIELD};
use dioxus::prelude::*;
use serde::Deserialize;

use super::script_with_element_id;
use super::shell::{rejection_reason, CurrentUser};
use crate::Route;

/// 영상 파일 입력 칸의 id. 올리기 스크립트가 이 칸에서 고른 파일을 읽는다.
const FILE_INPUT_ID: &str = "recording-file";

/// 고른 영상 파일과 제목을 [`RECORDING_UPLOAD_PATH`]로 올린다. 큰 파일을 wasm 메모리로 옮기지 않도록
/// 브라우저가 파일을 직접 보내게 한다(`FormData` + `XMLHttpRequest`, 진행률을 알 수 있다).
///
/// 받는 값: `[제목, 최대 바이트, 올리기 주소, 제목 칸 이름, 파일 칸 이름]`. 보내는 값([`UploadEvent`]):
/// `{kind: "progress", percent}`를 여러 번, 끝에 `done`·`rejected`·`no_file`·`too_large`·`failed` 중
/// 하나. `elementId`는 [`FILE_INPUT_ID`]다([`script_with_element_id`]).
const UPLOAD_JS: &str = r#"
const [title, maxBytes, url, titleField, fileField] = await dioxus.recv();
const input = document.getElementById(elementId);
const file = input && input.files && input.files[0];
if (!file) { dioxus.send({ kind: "no_file" }); return; }
if (file.size > maxBytes) { dioxus.send({ kind: "too_large" }); return; }
const form = new FormData();
// 서버는 제목 칸을 파일 칸보다 먼저 읽는다.
form.append(titleField, title);
form.append(fileField, file, file.name);
const xhr = new XMLHttpRequest();
let lastPercent = -1;
xhr.upload.onprogress = (event) => {
  if (!event.lengthComputable) return;
  const percent = Math.floor((event.loaded * 100) / event.total);
  if (percent !== lastPercent) {
    lastPercent = percent;
    dioxus.send({ kind: "progress", percent });
  }
};
xhr.onload = () => {
  let body = null;
  try { body = JSON.parse(xhr.responseText); } catch (e) {}
  if (xhr.status === 201 && body && typeof body.id === "number") {
    dioxus.send({ kind: "done", id: body.id });
  } else if (body && typeof body.message === "string") {
    dioxus.send({ kind: "rejected", message: body.message });
  } else {
    dioxus.send({ kind: "failed", status: xhr.status });
  }
};
xhr.onerror = () => dioxus.send({ kind: "failed", status: 0 });
xhr.open("POST", url);
xhr.send(form);
"#;

/// 올리기 스크립트가 보내는 메시지.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UploadEvent {
    Progress {
        percent: u8,
    },
    Done {
        id: i64,
    },
    /// 서버가 거절한 사유
    Rejected {
        message: String,
    },
    NoFile,
    TooLarge,
    /// 서버에 닿지 못했거나(0) 사유 없는 오류 응답
    Failed {
        status: u16,
    },
}

/// 녹화방송 등록 `/recordings/new`: 제목과 영상 파일을 골라 내 채널에 녹화방송을 올린다.
/// 사용자를 골라야 쓸 수 있다(인증은 없다, ADR 0001).
#[component]
pub fn NewRecording() -> Element {
    let CurrentUser(current) = use_context::<CurrentUser>();
    let user = current.read().clone();

    rsx! {
        section { class: "settings",
            h1 { "녹화방송 등록" }
            match user {
                None => rsx! {
                    p { class: "notice",
                        "사용자를 먼저 골라 주세요. 상단 오른쪽 사용자 메뉴(\"시청만 가능\")를 눌러 고르거나 새 닉네임으로 만들면 "
                        "그 사용자의 채널에 녹화방송을 등록할 수 있습니다. 비밀번호는 없습니다."
                    }
                },
                // 사용자를 바꾸면 입력을 새로 시작한다.
                Some(user) => rsx! {
                    RecordingForm { key: "{user.id}", nickname: user.nickname }
                },
            }
        }
    }
}

/// 제목, 영상 파일, 등록 버튼. 올리는 동안 진행률을 보여 주고, 등록되면 상세 페이지로 간다.
#[component]
fn RecordingForm(nickname: String) -> Element {
    let mut title = use_signal(String::new);
    // 올리는 중이면 진행률(%)
    let mut progress = use_signal(|| None::<u8>);
    let mut error = use_signal(|| None::<String>);
    let navigator = use_navigator();

    let submit = move || async move {
        if progress().is_some() {
            return;
        }
        error.set(None);
        progress.set(Some(0));
        match upload(title(), progress).await {
            Ok(id) => {
                navigator.push(Route::RecordingDetail { id });
            }
            Err(reason) => {
                progress.set(None);
                error.set(Some(reason));
            }
        }
    };

    let length = title.read().trim().chars().count();
    let count_class = if length > MAX_TITLE_CHARS {
        "title-count over"
    } else {
        "title-count"
    };
    let uploading = progress().is_some();
    let accept = RECORDING_EXTENSIONS
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(",");
    let formats = RECORDING_EXTENSIONS.join("·");

    rsx! {
        section { class: "settings-card",
            p { class: "muted",
                "{nickname}님의 채널에 등록합니다. 등록한 녹화방송은 홈의 녹화방송 목록에 나타나고 누구나 볼 수 있습니다."
            }
            div { class: "field",
                label { class: "field-label", r#for: "recording-title", "제목" }
                div { class: "field-row",
                    input {
                        id: "recording-title",
                        class: "title-input",
                        placeholder: "녹화방송 제목",
                        value: "{title}",
                        disabled: uploading,
                        oninput: move |event| {
                            title.set(event.value());
                            error.set(None);
                        },
                    }
                }
                p { class: "{count_class}", "{length}/{MAX_TITLE_CHARS}" }
            }
            div { class: "field",
                label { class: "field-label", r#for: FILE_INPUT_ID, "영상 파일" }
                input {
                    id: FILE_INPUT_ID,
                    class: "file-input",
                    r#type: "file",
                    accept,
                    disabled: uploading,
                    onchange: move |_| error.set(None),
                }
                p { class: "muted small", "{formats} 파일, 최대 {MAX_RECORDING_MB}MB" }
            }
            div { class: "form-actions",
                button {
                    class: "primary-button",
                    disabled: uploading,
                    onclick: move |_| async move { submit().await },
                    "등록"
                }
                if let Some(percent) = progress() {
                    span { class: "upload-progress", role: "status", "올리는 중… {percent}%" }
                }
            }
            if let Some(reason) = error() {
                p { class: "form-error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 고른 파일을 올리고 등록된 녹화방송 ID를 돌려준다. 실패하면 화면에 보여 줄 사유.
async fn upload(title: String, mut progress: Signal<Option<u8>>) -> Result<i64, String> {
    let mut eval = document::eval(&script_with_element_id(FILE_INPUT_ID, UPLOAD_JS));
    eval.send((
        title,
        MAX_RECORDING_BYTES,
        RECORDING_UPLOAD_PATH,
        UPLOAD_TITLE_FIELD,
        UPLOAD_FILE_FIELD,
    ))
    .map_err(|e| format!("올리기를 시작하지 못했습니다: {e}"))?;
    loop {
        let event = eval
            .recv::<UploadEvent>()
            .await
            .map_err(|e| format!("올리기 상태를 알 수 없습니다: {e}"))?;
        match event {
            UploadEvent::Progress { percent } => progress.set(Some(percent)),
            UploadEvent::Done { id } => return Ok(id),
            UploadEvent::Rejected { message } => return Err(message),
            UploadEvent::NoFile => return Err("영상 파일을 골라 주세요".to_string()),
            UploadEvent::TooLarge => {
                return Err(format!(
                    "영상 파일은 {MAX_RECORDING_MB}MB를 넘을 수 없습니다"
                ))
            }
            UploadEvent::Failed { status: 0 } => {
                return Err("서버에 연결하지 못했습니다".to_string())
            }
            UploadEvent::Failed { status } => {
                return Err(format!("올리지 못했습니다(HTTP {status})"))
            }
        }
    }
}

/// 녹화방송 상세 `/recordings/<ID>`: 영상을 처음부터 재생할 수 있다. 사용자를 고르지 않아도 볼 수 있다.
#[component]
pub fn RecordingDetail(id: ReadSignal<i64>) -> Element {
    // 서버 렌더링 때 불러온 정보를 HTML에 넣고, 클라이언트는 그 결과를 이어받는다.
    let loaded = use_server_future(move || api::recording_page(id()))?;
    let state = (*loaded.read()).clone();

    rsx! {
        section { class: "watch",
            match state {
                Some(Ok(Some(page))) => {
                    let registered = ago_text(page.server_now_ms - page.registered_at_ms);
                    rsx! {
                        // 다른 녹화방송으로 옮겨 가면 플레이어를 새로 만든다.
                        video {
                            key: "{page.id}",
                            class: "watch-player",
                            src: "{page.video_url}",
                            controls: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                        div { class: "watch-info",
                            h1 { class: "watch-title",
                                span { class: "recording-badge", "녹화" }
                                " {page.title}"
                            }
                            p { class: "watch-channel",
                                Link {
                                    to: Route::Watch {
                                        channel_id: page.channel_id.clone(),
                                    },
                                    "{page.channel_name}"
                                }
                            }
                            p { class: "muted", "{registered} 등록" }
                        }
                    }
                }
                Some(Ok(None)) => rsx! {
                    h1 { "녹화방송을 찾을 수 없습니다" }
                    p { class: "notice",
                        "주소의 녹화방송 번호({id})에 해당하는 녹화방송이 없습니다. "
                        Link { to: Route::Home {}, "홈에서 녹화방송을 고르세요." }
                    }
                },
                Some(Err(error)) => {
                    let reason = rejection_reason(error);
                    rsx! {
                        p { class: "form-error", role: "alert", "녹화방송 정보를 불러오지 못했습니다: {reason}" }
                    }
                }
                None => rsx! {},
            }
        }
    }
}

/// 등록 후 지난 시간: "방금 전", "12분 전", "3시간 전", "5일 전".
pub(super) fn ago_text(elapsed_ms: i64) -> String {
    let minutes = elapsed_ms.max(0) / 60_000;
    match (minutes / (60 * 24), minutes / 60, minutes) {
        (0, 0, 0) => "방금 전".to_string(),
        (0, 0, m) => format!("{m}분 전"),
        (0, h, _) => format!("{h}시간 전"),
        (d, _, _) => format!("{d}일 전"),
    }
}

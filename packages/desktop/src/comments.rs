//! 방송 하나(녹화방송·라이브 회차)의 댓글: 목록은 누구나 보고, 로그인하면 쓴다. 자기 댓글에만 "수정"·"삭제"
//! 버튼이 보인다. 버튼을 숨기는 것은 화면 편의일 뿐이고, 작성자 확인은 서버가 한다(403).
//! 쓰기·고치기·지우기가 성공하면 목록을 서버에서 다시 불러온다.

use dioxus::prelude::*;

use crate::auth::{use_auth, Auth};
use crate::client::{self, ApiError, Comment, Target, MAX_COMMENT_CHARS};
use crate::views::{ago_text, use_server};
use crate::Route;

/// 댓글 칸. `target`이 바뀌면(다른 방송) 목록을 다시 불러온다.
#[component]
pub fn Comments(target: ReadSignal<Target>) -> Element {
    let server = use_server();
    let auth = use_auth();
    // 쓰기·고치기·지우기가 성공하면 올려서 목록을 다시 불러온다.
    let version = use_signal(|| 0u32);
    let mut list = use_resource(move || {
        let server = server();
        let target = target();
        let _ = version();
        async move { client::comments(&server, target).await }
    });

    let me = auth.user.read().as_ref().map(|user| user.id);
    let body = match &*list.read() {
        None => rsx! { p { class: "notice", "댓글을 불러오는 중…" } },
        Some(Err(reason)) => rsx! {
            h2 { class: "comments-heading", "댓글" }
            p { class: "error", role: "alert", "댓글을 불러오지 못했습니다: {reason}" }
        },
        Some(Ok(loaded)) => {
            let count = loaded.comments.len();
            let server_now_ms = loaded.server_now_ms;
            rsx! {
                div { class: "heading",
                    h2 { class: "comments-heading", "댓글 {count}" }
                    button { class: "secondary small", onclick: move |_| list.restart(), "새로고침" }
                }
                if me.is_some() {
                    CommentWriter { target: target(), version }
                } else {
                    p { class: "notice login-required",
                        "로그인하면 댓글을 쓸 수 있습니다. "
                        Link { to: Route::Login {}, "로그인" }
                    }
                }
                if count == 0 {
                    p { class: "muted", "아직 댓글이 없습니다." }
                }
                ul { class: "comment-list",
                    for comment in loaded.comments.iter().cloned() {
                        CommentItem {
                            key: "{comment.id}",
                            mine: me == Some(comment.author_id),
                            comment,
                            server_now_ms,
                            version,
                        }
                    }
                }
            }
        }
    };

    rsx! {
        section { class: "comments", aria_label: "댓글", {body} }
    }
}

/// 화면에 보일 거절 사유. 401이면 세션이 끝났을 수 있으므로 다시 로그인하라고 덧붙인다.
fn reason_text(error: &ApiError) -> String {
    if error.is_unauthorized() {
        format!(
            "{} (로그인이 끝났을 수 있습니다. 로그아웃한 뒤 다시 로그인하세요.)",
            error.message
        )
    } else {
        error.message.clone()
    }
}

/// 입력 칸 글자 수: 서버처럼 앞뒤 공백을 지운 뒤 센다.
fn content_length(text: &str) -> usize {
    text.trim().chars().count()
}

/// 내용이 규칙(1~500자)에 맞는지. 서버가 다시 확인한다.
fn content_ok(text: &str) -> bool {
    (1..=MAX_COMMENT_CHARS).contains(&content_length(text))
}

/// 댓글 입력 칸과 "등록" 버튼.
#[component]
fn CommentWriter(target: Target, version: Signal<u32>) -> Element {
    let server = use_server();
    let auth: Auth = use_auth();
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut version = version;
    let length = content_length(&draft.read());

    let submit = move |_| async move {
        if busy() || !content_ok(&draft()) {
            return;
        }
        busy.set(true);
        let result =
            client::write_comment(&server(), auth.token().as_deref(), target, &draft()).await;
        busy.set(false);
        match result {
            Ok(_) => {
                draft.set(String::new());
                error.set(None);
                version += 1;
            }
            Err(e) => error.set(Some(reason_text(&e))),
        }
    };

    rsx! {
        div { class: "comment-writer",
            textarea {
                aria_label: "댓글 내용",
                placeholder: "댓글 남기기",
                rows: "2",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
            }
            div { class: "comment-writer-row",
                span { class: if length > MAX_COMMENT_CHARS { "count over" } else { "count" },
                    "{length}/{MAX_COMMENT_CHARS}"
                }
                button {
                    disabled: busy() || length == 0 || length > MAX_COMMENT_CHARS,
                    onclick: submit,
                    "등록"
                }
            }
            if let Some(reason) = error() {
                p { class: "error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 댓글 하나. `mine`이면 "수정"·"삭제" 버튼을 보인다.
#[component]
fn CommentItem(comment: Comment, mine: bool, server_now_ms: i64, version: Signal<u32>) -> Element {
    let server = use_server();
    let auth = use_auth();
    let mut version = version;
    let mut editing = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let written = ago_text(server_now_ms - comment.created_at_ms);
    let id = comment.id;

    let save = move |_| async move {
        let Some(content) = editing() else { return };
        if busy() {
            return;
        }
        if !content_ok(&content) {
            error.set(Some(format!(
                "댓글은 1~{MAX_COMMENT_CHARS}자로 적어 주세요."
            )));
            return;
        }
        busy.set(true);
        let result = client::edit_comment(&server(), auth.token().as_deref(), id, &content).await;
        busy.set(false);
        match result {
            Ok(_) => {
                editing.set(None);
                error.set(None);
                version += 1;
            }
            Err(e) => error.set(Some(reason_text(&e))),
        }
    };

    let delete = move |_| async move {
        if busy() {
            return;
        }
        busy.set(true);
        let result = client::delete_comment(&server(), auth.token().as_deref(), id).await;
        busy.set(false);
        match result {
            Ok(()) => {
                error.set(None);
                version += 1;
            }
            Err(e) => error.set(Some(reason_text(&e))),
        }
    };

    rsx! {
        li { class: "comment",
            div { class: "comment-meta",
                span { class: "comment-author", "{comment.author_nickname}" }
                span { class: "muted small", " · {written}" }
                if comment.edited {
                    span { class: "muted small", " · 수정됨" }
                }
            }
            match editing() {
                Some(draft) => rsx! {
                    textarea {
                        aria_label: "댓글 고치기",
                        rows: "2",
                        value: "{draft}",
                        oninput: move |event| editing.set(Some(event.value())),
                    }
                    div { class: "comment-actions",
                        button { class: "small", disabled: busy(), onclick: save, "저장" }
                        button {
                            class: "secondary small",
                            onclick: move |_| {
                                editing.set(None);
                                error.set(None);
                            },
                            "취소"
                        }
                    }
                },
                None => rsx! {
                    p { class: "comment-content", "{comment.content}" }
                    if mine {
                        div { class: "comment-actions",
                            button {
                                class: "link-button",
                                onclick: {
                                    let content = comment.content.clone();
                                    move |_| editing.set(Some(content.clone()))
                                },
                                "수정"
                            }
                            button {
                                class: "link-button danger",
                                disabled: busy(),
                                onclick: delete,
                                "삭제"
                            }
                        }
                    }
                },
            }
            if let Some(reason) = error() {
                p { class: "error", role: "alert", "{reason}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 댓글_내용은_앞뒤_공백을_빼고_1자에서_500자() {
        assert!(!content_ok(""));
        assert!(!content_ok("   \n "));
        assert!(content_ok("  안녕  "));
        assert!(content_ok(&"가".repeat(MAX_COMMENT_CHARS)));
        assert!(!content_ok(&"가".repeat(MAX_COMMENT_CHARS + 1)));
    }

    #[test]
    fn 로그인_거절에는_다시_로그인하라고_덧붙인다() {
        let unauthorized = ApiError {
            status: Some(401),
            message: "로그인이 필요합니다".into(),
        };
        assert!(reason_text(&unauthorized).starts_with("로그인이 필요합니다 ("));
        let forbidden = ApiError {
            status: Some(403),
            message: "댓글을 쓴 사람만 고치거나 지울 수 있습니다".into(),
        };
        assert_eq!(
            reason_text(&forbidden),
            "댓글을 쓴 사람만 고치거나 지울 수 있습니다"
        );
    }
}

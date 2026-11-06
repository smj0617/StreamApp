use api::dto::{BroadcastTarget, Comment};
use api::rules::MAX_COMMENT_CHARS;
use dioxus::prelude::*;

use super::recordings::ago_text;
use super::shell::{rejection_reason, CurrentUser, LoginRequired};

/// 방송 하나(녹화방송·라이브)의 댓글: 목록은 누구나 보고, 로그인하면 쓴다. 자기 댓글에만 "수정"·"삭제"
/// 버튼이 보인다. 버튼을 숨기는 것은 화면 편의일 뿐이고, 작성자 확인은 서버가 한다.
#[component]
pub fn Comments(target: BroadcastTarget) -> Element {
    // 서버 렌더링 때 불러온 목록을 HTML에 넣고, 클라이언트는 그 결과를 이어받는다.
    let loaded = use_server_future(move || async move {
        match target {
            BroadcastTarget::Recording(id) => api::recording_comments(id).await,
            BroadcastTarget::Live(id) => api::live_comments(id).await,
        }
    })?;
    let state = (*loaded.read()).clone();

    rsx! {
        section { class: "comments", aria_label: "댓글",
            match state {
                Some(Ok(list)) => rsx! {
                    CommentBoard { comments: list.comments, server_now_ms: list.server_now_ms, target }
                },
                Some(Err(error)) => {
                    let reason = rejection_reason(error);
                    rsx! {
                        h2 { class: "comments-heading", "댓글" }
                        p { class: "form-error", role: "alert", "댓글을 불러오지 못했습니다: {reason}" }
                    }
                }
                None => rsx! {},
            }
        }
    }
}

/// 불러온 댓글을 복사해 두고, 쓰기·고치기·지우기 결과를 여기에 바로 반영한다(목록을 다시 불러오지 않는다).
#[component]
fn CommentBoard(comments: Vec<Comment>, server_now_ms: i64, target: BroadcastTarget) -> Element {
    let CurrentUser(current) = use_context::<CurrentUser>();
    let list = use_signal(|| comments);
    let me = current.read().as_ref().map(|user| user.id);
    let count = list.read().len();

    rsx! {
        h2 { class: "comments-heading", "댓글 {count}" }
        if me.is_some() {
            CommentWriter { target, list }
        } else {
            LoginRequired { what: "댓글을 쓸 수 있습니다." }
        }
        if count == 0 {
            p { class: "muted", "아직 댓글이 없습니다." }
        }
        ul { class: "comment-list",
            for comment in list.read().iter().cloned() {
                CommentItem {
                    key: "{comment.id}",
                    mine: me == Some(comment.author_id),
                    comment,
                    server_now_ms,
                    list,
                }
            }
        }
    }
}

/// 댓글 입력 칸과 "등록" 버튼.
#[component]
fn CommentWriter(target: BroadcastTarget, list: Signal<Vec<Comment>>) -> Element {
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut list = list;
    let length = draft.read().trim().chars().count();

    let submit = move || async move {
        match api::write_comment(target, draft()).await {
            Ok(comment) => {
                list.write().push(comment);
                draft.set(String::new());
                error.set(None);
            }
            Err(e) => error.set(Some(rejection_reason(e))),
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
                span { class: if length > MAX_COMMENT_CHARS { "chat-count over" } else { "chat-count" },
                    "{length}/{MAX_COMMENT_CHARS}"
                }
                button {
                    disabled: length == 0 || length > MAX_COMMENT_CHARS,
                    onclick: move |_| async move { submit().await },
                    "등록"
                }
            }
            if let Some(reason) = error() {
                p { class: "form-error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 댓글 하나. `mine`이면 "수정"·"삭제" 버튼을 보인다.
#[component]
fn CommentItem(
    comment: Comment,
    mine: bool,
    server_now_ms: i64,
    list: Signal<Vec<Comment>>,
) -> Element {
    let mut list = list;
    let mut editing = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let written = ago_text(server_now_ms - comment.created_at_ms);
    let id = comment.id;

    let save = move || async move {
        let Some(content) = editing() else { return };
        match api::edit_comment(id, content).await {
            Ok(edited) => {
                if let Some(slot) = list.write().iter_mut().find(|c| c.id == id) {
                    *slot = edited;
                }
                editing.set(None);
                error.set(None);
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
    };

    let delete = move || async move {
        match api::delete_comment(id).await {
            Ok(()) => list.write().retain(|c| c.id != id),
            Err(e) => error.set(Some(rejection_reason(e))),
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
                        button { onclick: move |_| async move { save().await }, "저장" }
                        button { class: "secondary", onclick: move |_| editing.set(None), "취소" }
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
                                onclick: move |_| async move { delete().await },
                                "삭제"
                            }
                        }
                    }
                },
            }
            if let Some(reason) = error() {
                p { class: "form-error", role: "alert", "{reason}" }
            }
        }
    }
}

//! 라이브 HLS 중계: 웹뷰 안의 hls.js가 받는 재생 목록·조각을 앱(Rust)이 대신 받아 넘긴다.
//!
//! 웹뷰 페이지의 출처는 `http://dioxus.index.html`(Windows)·`dioxus://index.html`(macOS)이라, hls.js가 HLS 서버에
//! 직접 요청하면 CORS에 막힌다(MediaMTX는 `hlsAllowOrigins`에 적은 웹 앱 출처만 허용한다). 그래서 웹뷰는 같은
//! 출처의 `/hlsproxy/<원래 폴더 주소의 16진수>/<파일>` 주소를 받고, 이 모듈이 원래 주소로 바꿔 가져온다.
//! 재생 목록 안의 상대 주소(`stream.m3u8`, 조각 파일)도 같은 폴더 아래로 풀리므로 그대로 중계된다.
//!
//! 네이티브 HLS로 재생하는 macOS는 `<video src>`로 원래 주소를 바로 쓰므로(미디어 요소는 CORS를 보지 않는다)
//! 이 중계를 거치지 않는다.

use dioxus::desktop::wry::http::{header, Response, StatusCode};
use dioxus::desktop::{AssetRequest, RequestAsyncResponder};
use url::Url;

use crate::client::HTTP;

/// 웹뷰 안 중계 주소의 첫 경로 이름(`use_asset_handler`에 등록하는 이름).
pub const PROXY_NAME: &str = "hlsproxy";

/// 재생 주소를 웹뷰 안의 중계 주소로 바꾼다.
/// `https://example.com/hls/ch1/index.m3u8` → `/hlsproxy/<hex("https://example.com/hls/ch1/")>/index.m3u8`.
pub fn proxied_path(playback_url: &str) -> Option<String> {
    let url = Url::parse(playback_url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let file = url.path_segments()?.next_back()?.to_string();
    let dir = url.join("./").ok()?;
    let mut path = format!("/{PROXY_NAME}/{}/{file}", hex_encode(dir.as_str()));
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    Some(path)
}

/// 웹뷰가 요청한 중계 주소(경로와 쿼리)를 원래 주소로 되돌린다. 원래 폴더 밖으로 나가는 주소는 받지 않는다.
pub fn upstream_url(path_and_query: &str) -> Option<Url> {
    let rest = path_and_query
        .strip_prefix('/')?
        .strip_prefix(PROXY_NAME)?
        .strip_prefix('/')?;
    let (encoded_dir, file) = rest.split_once('/')?;
    let dir = Url::parse(&String::from_utf8(hex_decode(encoded_dir)?).ok()?).ok()?;
    if !matches!(dir.scheme(), "http" | "https") {
        return None;
    }
    let target = dir.join(file).ok()?;
    let inside = target.origin() == dir.origin() && target.path().starts_with(dir.path());
    inside.then_some(target)
}

/// `use_asset_handler`로 등록하는 처리기. 원래 주소에서 받아 상태·Content-Type·본문을 그대로 돌려준다.
pub fn handle(request: AssetRequest, responder: RequestAsyncResponder) {
    let target = request
        .uri()
        .path_and_query()
        .and_then(|pq| upstream_url(pq.as_str()));
    // 데스크톱 런처가 만든 tokio 런타임 안에서 불린다. 응답은 다른 스레드에서 보내도 된다.
    tokio::spawn(async move {
        let response = match target {
            Some(url) => fetch(url).await,
            None => plain(StatusCode::BAD_REQUEST, "잘못된 중계 주소"),
        };
        responder.respond(response);
    });
}

async fn fetch(url: Url) -> Response<Vec<u8>> {
    let upstream = match HTTP.get(url).send().await {
        Ok(response) => response,
        Err(e) => {
            return plain(
                StatusCode::BAD_GATEWAY,
                &format!("HLS 서버에 연결하지 못했습니다: {e}"),
            )
        }
    };
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    match upstream.bytes().await {
        Ok(body) => Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body.to_vec())
            .unwrap_or_else(|_| plain(StatusCode::BAD_GATEWAY, "응답을 만들지 못했습니다")),
        Err(e) => plain(
            StatusCode::BAD_GATEWAY,
            &format!("HLS 응답을 받지 못했습니다: {e}"),
        ),
    }
}

fn plain(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    let mut response = Response::new(message.as_bytes().to_vec());
    *response.status_mut() = status;
    response
}

fn hex_encode(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 재생_주소를_중계_주소로_바꾸고_되돌린다() {
        let path = proxied_path("https://example.com/hls/ch1/index.m3u8").unwrap();
        assert!(path.starts_with("/hlsproxy/"));
        assert!(path.ends_with("/index.m3u8"));
        assert_eq!(
            upstream_url(&path).unwrap().as_str(),
            "https://example.com/hls/ch1/index.m3u8"
        );
    }

    #[test]
    fn 재생_목록_안의_상대_주소와_쿼리도_같은_폴더로_간다() {
        let path = proxied_path("http://localhost:8888/ch1/index.m3u8").unwrap();
        let dir = path.trim_end_matches("index.m3u8");
        let segment = format!("{dir}stream.m3u8?_HLS_msn=3&_HLS_part=1");
        assert_eq!(
            upstream_url(&segment).unwrap().as_str(),
            "http://localhost:8888/ch1/stream.m3u8?_HLS_msn=3&_HLS_part=1"
        );
    }

    #[test]
    fn 폴더_밖이나_잘못된_주소는_받지_않는다() {
        let path = proxied_path("http://localhost:8888/ch1/index.m3u8").unwrap();
        let dir = path.trim_end_matches("index.m3u8");
        assert!(upstream_url(&format!("{dir}../other/index.m3u8")).is_none());
        assert!(upstream_url(&format!("{dir}http://evil.example/x")).is_none());
        assert!(upstream_url("/hlsproxy/zz/index.m3u8").is_none());
        assert!(upstream_url("/other/x").is_none());
        assert!(proxied_path("file:///c:/x.m3u8").is_none());
    }
}

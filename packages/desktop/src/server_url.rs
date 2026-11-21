//! 데스크톱 클라이언트가 붙는 백엔드(서버) 주소.
//!
//! 기본값은 빌드할 때 환경변수 `STREAMAPP_SERVER_URL`로 정하고(없거나 비었으면 `http://localhost:8090`),
//! 실행 중에는 설정 화면에서 바꿀 수 있다. 바꾼 주소는 사용자 설정 폴더의 파일에 저장해 다음 실행에도 쓴다.

use std::path::PathBuf;

use url::Url;

/// 빌드할 때 넣은 서버 주소. 릴리스 워크플로가 리포지토리 변수 `STREAMAPP_SERVER_URL`로 채운다.
const BUILT_SERVER_URL: Option<&str> = option_env!("STREAMAPP_SERVER_URL");
/// 빌드 값이 없을 때의 서버 주소(로컬 개발: `scripts/dev.ps1 -Port 8090`).
const FALLBACK_SERVER_URL: &str = "http://localhost:8090";

/// 확인을 마친 서버 주소. `http`·`https`만 받고, 끝의 `/`는 뗀다(예: `https://example.com`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerUrl(String);

impl ServerUrl {
    /// 입력한 주소를 확인한다. 앞뒤 공백은 지운다. 틀리면 화면에 보여 줄 사유를 돌려준다.
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("서버 주소를 입력하세요.".to_string());
        }
        let url = Url::parse(input).map_err(|_| {
            format!("주소 형식이 아닙니다: {input} (예: https://example.com 또는 http://localhost:8090)")
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("http:// 또는 https://로 시작해야 합니다.".to_string());
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err("주소에 호스트가 없습니다.".to_string());
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("주소에 ?나 #는 넣지 않습니다.".to_string());
        }
        Ok(Self(url.as_str().trim_end_matches('/').to_string()))
    }

    /// 이 앱의 기본 서버 주소: 빌드할 때 넣은 값, 없거나 틀리면 `http://localhost:8090`.
    pub fn built_default() -> Self {
        BUILT_SERVER_URL
            .and_then(|value| Self::parse(value).ok())
            .unwrap_or_else(|| Self(FALLBACK_SERVER_URL.to_string()))
    }

    /// 시작할 때 쓸 주소: 저장한 주소가 있으면 그것, 없으면 [`Self::built_default`].
    pub fn load() -> Self {
        settings_file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|saved| Self::parse(&saved).ok())
            .unwrap_or_else(Self::built_default)
    }

    /// 주소를 설정 파일에 저장한다. 실패하면 사유를 돌려준다(주소는 이번 실행 동안만 쓴다).
    pub fn save(&self) -> Result<(), String> {
        let path = settings_file().ok_or("설정 폴더를 찾지 못했습니다.")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("설정 폴더를 만들지 못했습니다: {e}"))?;
        }
        std::fs::write(&path, &self.0).map_err(|e| format!("설정을 저장하지 못했습니다: {e}"))
    }

    /// 저장한 주소를 지운다(다음 실행부터 기본 주소).
    pub fn forget_saved() {
        if let Some(path) = settings_file() {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 서버 안의 경로(`/`로 시작)를 전체 주소로 만든다. 예: `/api/recordings`.
    pub fn join(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{path}", self.0)
        } else {
            format!("{}/{path}", self.0)
        }
    }

    /// 서버가 준 주소를 전체 주소로 만든다. 이미 `http(s)://`로 시작하면 그대로, 아니면 서버 주소 기준.
    /// 녹화방송 영상 주소(`/media/recordings/<파일>`)와 라이브 재생 주소에 쓴다.
    pub fn absolutize(&self, url: &str) -> String {
        match Url::parse(url) {
            Ok(absolute) if matches!(absolute.scheme(), "http" | "https") => url.to_string(),
            _ => self.join(url),
        }
    }

    /// 서버가 재생 주소를 주지 못할 때 쓰는 라이브 HLS 주소 규칙: `<서버>/hls/<채널ID>/index.m3u8`.
    /// 배포 구성(리버스 프록시가 `/hls/`를 MediaMTX로 넘김)을 가정한다.
    pub fn fallback_hls_url(&self, channel_id: &str) -> String {
        self.join(&format!("/hls/{channel_id}/index.m3u8"))
    }
}

/// 서버 주소를 저장하는 파일. Windows `%APPDATA%\StreamApp\server-url.txt`,
/// macOS `~/Library/Application Support/StreamApp/server-url.txt`, 그 밖 `~/.config/StreamApp/server-url.txt`.
pub fn settings_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        }
    };
    Some(base.join("StreamApp").join("server-url.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 주소는_끝의_빗금을_떼고_공백을_지운다() {
        let url = ServerUrl::parse("  https://stream.example.com/ ").unwrap();
        assert_eq!(url.as_str(), "https://stream.example.com");
        assert_eq!(
            ServerUrl::parse("http://localhost:8090").unwrap().as_str(),
            "http://localhost:8090"
        );
    }

    #[test]
    fn http와_https가_아닌_주소는_받지_않는다() {
        assert!(ServerUrl::parse("").is_err());
        assert!(ServerUrl::parse("example.com").is_err());
        assert!(ServerUrl::parse("ftp://example.com").is_err());
        assert!(ServerUrl::parse("https://example.com/?a=1").is_err());
    }

    #[test]
    fn 서버_경로와_상대_주소를_전체_주소로_만든다() {
        let server = ServerUrl::parse("https://stream.example.com").unwrap();
        assert_eq!(
            server.join("/api/live-channels"),
            "https://stream.example.com/api/live-channels"
        );
        assert_eq!(
            server.absolutize("/media/recordings/a.mp4"),
            "https://stream.example.com/media/recordings/a.mp4"
        );
        assert_eq!(
            server.absolutize("http://localhost:8888/ch/index.m3u8"),
            "http://localhost:8888/ch/index.m3u8"
        );
        assert_eq!(
            server.fallback_hls_url("ch1"),
            "https://stream.example.com/hls/ch1/index.m3u8"
        );
    }

    #[test]
    fn 기본_주소는_항상_올바르다() {
        let url = ServerUrl::built_default();
        assert!(ServerUrl::parse(url.as_str()).is_ok());
    }
}

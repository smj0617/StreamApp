//! 로그인 세션 저장. 서버가 로그인 응답의 쿠키(`streamapp_session`)로 준 세션 토큰을 앱이 직접 들고 있다가
//! 요청마다 `Cookie` 헤더로 싣는다([`crate::client`]).
//!
//! 다음 실행에도 로그인을 유지하려고 토큰을 서버 주소와 함께 앱 데이터 폴더의 `session.txt`에 저장한다
//! (서버 주소 파일 `server-url.txt`와 같은 폴더). 비밀번호는 저장하지 않는다. 저장한 서버 주소가 지금 서버와
//! 다르면 그 세션은 버린다(다른 서버에 토큰을 보내지 않는다). 로그아웃하면 파일을 지운다.
//!
//! 위험: 이 파일을 읽을 수 있는 사람(같은 OS 계정으로 도는 프로그램 포함)은 세션 기한(30일) 동안 그 계정으로
//! 행동할 수 있다. 로그아웃하면 서버의 세션도 지워져 파일이 남아 있어도 더는 쓸 수 없다.

use std::path::PathBuf;

use crate::server_url::{self, ServerUrl};

/// 한 서버에서 받은 세션 토큰.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub server: ServerUrl,
    pub token: String,
}

impl Session {
    /// 저장한 세션을 읽는다. 없거나, 형식이 틀리거나, 다른 서버의 세션이면 `None`이고 다른 서버의 것은 지운다.
    pub fn load(server: &ServerUrl) -> Option<Self> {
        let text = std::fs::read_to_string(session_file()?).ok()?;
        match Self::decode(&text) {
            Some(saved) if &saved.server == server => Some(saved),
            _ => {
                Self::forget_saved();
                None
            }
        }
    }

    /// 세션을 파일에 저장한다. 실패하면 사유를 돌려준다(세션은 이번 실행 동안만 쓴다).
    pub fn save(&self) -> Result<(), String> {
        let path = session_file().ok_or("설정 폴더를 찾지 못했습니다.")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("설정 폴더를 만들지 못했습니다: {e}"))?;
        }
        write_private(&path, &self.encode())
            .map_err(|e| format!("로그인 상태를 저장하지 못했습니다: {e}"))
    }

    /// 저장한 세션 파일을 지운다(로그아웃, 서버 주소 바꾸기, 세션 끝남).
    pub fn forget_saved() {
        if let Some(path) = session_file() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// 파일 내용: 첫 줄 서버 주소, 둘째 줄 토큰.
    fn encode(&self) -> String {
        format!("{}\n{}\n", self.server.as_str(), self.token)
    }

    fn decode(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let server = ServerUrl::parse(lines.next()?).ok()?;
        let token = lines.next()?.trim();
        if token.is_empty() || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        Some(Self {
            server,
            token: token.to_string(),
        })
    }
}

/// 세션 파일: 서버 주소 파일과 같은 폴더의 `session.txt`.
pub fn session_file() -> Option<PathBuf> {
    Some(server_url::settings_file()?.with_file_name("session.txt"))
}

/// 파일을 쓴다. Unix 계열(macOS)에서는 소유자만 읽고 쓰게(0600) 만든다. Windows의 `%APPDATA%`는 이미
/// 사용자 본인만 접근할 수 있는 폴더다.
fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 세션_파일_내용을_읽고_쓴다() {
        let session = Session {
            server: ServerUrl::parse("https://stream.example.com").unwrap(),
            token: "abcDEF123".to_string(),
        };
        assert_eq!(Session::decode(&session.encode()), Some(session));
    }

    #[test]
    fn 틀린_세션_파일은_버린다() {
        assert_eq!(Session::decode(""), None);
        assert_eq!(Session::decode("https://stream.example.com\n"), None);
        assert_eq!(Session::decode("주소 아님\nabc\n"), None);
        assert_eq!(
            Session::decode("https://stream.example.com\nabc; Path=/\n"),
            None
        );
    }

    #[test]
    fn 세션_파일은_서버_주소_파일과_같은_폴더에_있다() {
        if let (Some(session), Some(server)) = (session_file(), server_url::settings_file()) {
            assert_eq!(session.parent(), server.parent());
        }
    }
}

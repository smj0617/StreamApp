//! 데스크톱 앱 다운로드 페이지(`/download`). OS별 설치 파일 링크, 지원 아키텍처, 설치·실행 방법을 보여 준다.
//!
//! 설치 파일은 GitHub Releases에 고정된 파일 이름으로 올린다(`.github/workflows/release-desktop.yml`).
//! 링크는 "가장 최근 릴리스의 그 이름 파일"을 가리키므로, 새 버전을 올려도 이 페이지를 고칠 필요가 없다.

use dioxus::prelude::*;

/// 설치 파일 링크의 앞부분. 빌드할 때 환경변수 `STREAMAPP_DOWNLOAD_BASE_URL`로 바꿀 수 있다
/// (예: 저장소를 옮겼거나 다른 곳에 파일을 올릴 때). 끝에 `/`를 붙이지 않는다.
pub const DOWNLOAD_BASE_URL: &str = match option_env!("STREAMAPP_DOWNLOAD_BASE_URL") {
    Some(url) => url,
    None => "https://github.com/smj0617/StreamApp/releases/latest/download",
};

/// 모든 릴리스 목록(이전 버전·변경 내역).
const RELEASES_URL: &str = "https://github.com/smj0617/StreamApp/releases";

/// 설치 파일 하나. 파일 이름은 릴리스 워크플로가 올리는 이름과 같아야 한다.
struct Package {
    os: &'static str,
    arch: &'static str,
    kind: &'static str,
    file: &'static str,
}

const PACKAGES: [Package; 4] = [
    Package {
        os: "Windows 10·11",
        arch: "x64 (Intel·AMD 64비트)",
        kind: "설치 프로그램(.exe, 권장)",
        file: "StreamApp-windows-x64-setup.exe",
    },
    Package {
        os: "Windows 10·11",
        arch: "x64 (Intel·AMD 64비트)",
        kind: "Windows Installer(.msi)",
        file: "StreamApp-windows-x64.msi",
    },
    Package {
        os: "macOS 11 이상",
        arch: "Apple Silicon (M1·M2·M3·M4, arm64)",
        kind: "디스크 이미지(.dmg)",
        file: "StreamApp-macos-arm64.dmg",
    },
    Package {
        os: "macOS 11 이상",
        arch: "Intel (x86_64)",
        kind: "디스크 이미지(.dmg)",
        file: "StreamApp-macos-x64.dmg",
    },
];

fn download_url(file: &str) -> String {
    format!("{}/{file}", DOWNLOAD_BASE_URL.trim_end_matches('/'))
}

#[component]
pub fn Download() -> Element {
    rsx! {
        section { class: "download",
            h1 { "StreamApp 데스크톱 앱" }
            p {
                "Windows와 macOS에서 라이브와 녹화방송 목록을 보고 바로 재생하는 앱입니다. "
                "채팅·댓글·후원은 앱의 \"웹에서 열기\"로 이 사이트에서 이용합니다."
            }

            h2 { "내려받기" }
            table { class: "download-table",
                thead {
                    tr {
                        th { "OS" }
                        th { "지원 아키텍처" }
                        th { "파일" }
                    }
                }
                tbody {
                    for package in PACKAGES.iter() {
                        tr { key: "{package.file}",
                            td { "{package.os}" }
                            td { "{package.arch}" }
                            td {
                                a { href: download_url(package.file), "{package.kind}" }
                                br {}
                                code { class: "small", "{package.file}" }
                            }
                        }
                    }
                }
            }
            p { class: "muted small",
                "이전 버전과 변경 내역: "
                a { href: RELEASES_URL, "GitHub Releases" }
                ". Windows ARM64와 Linux용 파일은 아직 없습니다."
            }

            h2 { "Windows에서 설치·실행" }
            ol {
                li { "StreamApp-windows-x64-setup.exe를 내려받아 실행합니다(관리자 권한 없이 현재 사용자에게 설치됩니다)." }
                li {
                    "코드 서명을 하지 않은 앱이라 \"Windows의 PC 보호\"(SmartScreen) 창이 뜨면 "
                    strong { "추가 정보 → 실행" }
                    "을 누릅니다."
                }
                li { "설치가 끝나면 시작 메뉴의 StreamApp을 실행합니다." }
                li {
                    "WebView2 런타임이 필요합니다. Windows 11과 최신 Windows 10에는 들어 있고, 없으면 설치 프로그램이 내려받습니다."
                }
            }

            h2 { "macOS에서 설치·실행" }
            ol {
                li { "내 Mac의 칩에 맞는 .dmg를 내려받습니다(Apple 메뉴 → 이 Mac에 관하여 → 칩/프로세서)." }
                li { ".dmg를 열고 StreamApp을 응용 프로그램(Applications) 폴더로 끌어 놓습니다." }
                li {
                    "Apple 공증을 받지 않은 앱이라 처음 실행할 때 막힙니다. Finder에서 StreamApp을 "
                    strong { "Control-클릭(우클릭) → 열기" }
                    "를 누르고 다시 \"열기\"를 고릅니다. macOS 15 이상에서는 한 번 실행을 시도한 뒤 "
                    strong { "시스템 설정 → 개인정보 보호 및 보안 → 그래도 열기" }
                    "를 누릅니다."
                }
                li {
                    "\"손상되었기 때문에 열 수 없습니다\"가 뜨면 터미널에서 다음을 실행한 뒤 다시 엽니다: "
                    code { "xattr -dr com.apple.quarantine /Applications/StreamApp.app" }
                }
            }

            h2 { "처음 실행한 뒤" }
            p {
                "앱은 이 사이트의 서버에 연결하도록 만들어져 있습니다. 다른 서버를 쓰려면 앱 위쪽의 "
                strong { "서버 설정" }
                "에서 주소를 바꿉니다."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 내려받기_주소는_최신_릴리스의_고정_파일_이름이다() {
        if option_env!("STREAMAPP_DOWNLOAD_BASE_URL").is_none() {
            assert_eq!(
                download_url("StreamApp-macos-arm64.dmg"),
                "https://github.com/smj0617/StreamApp/releases/latest/download/StreamApp-macos-arm64.dmg"
            );
        }
        assert!(PACKAGES.iter().all(|p| p.file.starts_with("StreamApp-")));
    }
}

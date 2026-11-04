//! 휴대폰 방송(브라우저 송출): 웹과 서버가 함께 쓰는 주소.
//!
//! 휴대폰 브라우저는 카메라·마이크 영상을 WebRTC로 보낸다. 송출 시작 요청(WHIP)은 앱이 받아
//! 현재 사용자 채널의 송출 키를 붙여 MediaMTX로 넘기고([`crate::whip`]), 영상·소리는 휴대폰이 MediaMTX로
//! 직접 보낸다(ADR 0004). 송출이 시작되고 끝나는 것은 OBS 송출과 똑같이 MediaMTX 훅이 라이브로 반영한다.

/// 휴대폰 방송 WHIP 주소. 브라우저가 SDP 제안(`application/sdp`)을 POST하면 201과 SDP 응답, 그리고
/// 송출을 끝낼 때 DELETE할 주소(`Location: <BROADCAST_WHIP_PATH>/<세션>`)를 돌려준다. 거절하면 400(사용자를
/// 고르지 않음), 409(미디어 서버가 거절, 예: 이미 송출 중), 502(미디어 서버에 연결하지 못함)와
/// `{"message": "<사유>"}`.
pub const BROADCAST_WHIP_PATH: &str = "/api/broadcast/whip";

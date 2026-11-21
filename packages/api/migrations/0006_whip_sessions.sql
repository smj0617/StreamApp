-- 휴대폰 방송 송출 세션 (과제 ⑤): 앱이 MediaMTX로 넘겨 시작한 WHIP 세션이 어느 채널의 것인지.
-- 송출 종료 요청이 그 채널 주인의 것인지 확인한다(`whip` 모듈). 이전에는 앱 인스턴스의 메모리에 있었으나,
-- 앱이 여럿이면 종료 요청이 시작 요청과 다른 인스턴스로 갈 수 있어 공용 DB로 옮긴다.
CREATE TABLE whip_sessions (
    session_id    TEXT    PRIMARY KEY,                         -- MediaMTX가 준 세션 ID(UUID 모양)
    channel_id    TEXT    NOT NULL REFERENCES channels(id),    -- 송출을 시작한 채널
    created_at_ms INTEGER NOT NULL                             -- 유닉스 밀리초
);

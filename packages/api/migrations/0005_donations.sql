-- 후원 (Donation, 과제 ④): 로그인한 사용자가 방송(녹화방송·라이브)을 보며 그 채널 주인에게 보낸 돈.
-- 결제는 토스페이먼츠 테스트 모드로 하고, 서버가 결제 승인을 확인한 뒤에만 확정한다 (ADR 0006).
--
-- 한 행 = 주문 하나. 결제창을 열기 전에 'pending'으로 만들고, 승인을 확인하면 'confirmed'가 된다.
-- 같은 승인 통지(성공 페이지 새로고침, 웹훅 재전송)가 다시 와도 확정은 한 번뿐이다:
--   - order_id·payment_key가 UNIQUE라 한 결제가 두 행이 될 수 없고,
--   - 확정은 "아직 confirmed가 아닌 행"만 바꾸는 조건부 UPDATE라 두 번째 통지는 아무것도 바꾸지 않는다.
CREATE TABLE donations (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id             TEXT    NOT NULL UNIQUE,                 -- 결제 서비스에 넘기는 주문번호(서버가 만든 무작위 값)
    donor_id             INTEGER NOT NULL REFERENCES users(id),   -- 후원한 사용자
    recipient_channel_id TEXT    NOT NULL REFERENCES channels(id),-- 받는 채널(방송 주인). 서버가 방송에서 정한다
    recording_id         INTEGER REFERENCES recordings(id) ON DELETE SET NULL, -- 녹화방송이 지워져도 후원 기록은 남는다
    live_id              INTEGER REFERENCES lives(id),
    amount               INTEGER NOT NULL CHECK (amount > 0),     -- 원(KRW). 서버가 주문을 만들 때 정한 금액
    message              TEXT    NOT NULL DEFAULT '',             -- 후원 메시지(앞뒤 공백을 지움, 비어 있을 수 있다)
    status               TEXT    NOT NULL DEFAULT 'pending'
                         CHECK (status IN ('pending', 'confirmed', 'canceled', 'failed')),
    payment_key          TEXT    UNIQUE,                          -- 결제 서비스의 결제 키. 확정할 때 남긴다
    failure_reason       TEXT,                                    -- 'failed'·'canceled'가 된 이유
    created_at_ms        INTEGER NOT NULL,                        -- 유닉스 밀리초
    confirmed_at_ms      INTEGER,                                 -- 확정한 시각, 확정 전에는 NULL
    CHECK (recording_id IS NULL OR live_id IS NULL),
    CHECK ((status = 'confirmed') = (payment_key IS NOT NULL AND confirmed_at_ms IS NOT NULL))
);
CREATE INDEX donations_recording ON donations (recording_id, status);
CREATE INDEX donations_live ON donations (live_id, status);

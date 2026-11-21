//! 어느 앱 인스턴스(파드)가 답했는지 응답 헤더 [`INSTANCE_HEADER`]에 붙인다(과제 ⑤). 여러 인스턴스 앞의
//! 요청 분산을 `curl -I`나 부하 스크립트로 확인할 때 쓴다.
//!
//! 환경변수 [`INSTANCE_NAME_ENV`]가 있을 때만 붙인다. 쿠버네티스 매니페스트(`k8s/app/streamapp.yaml`)는
//! Downward API로 파드 이름을 넣는다. 단일 서버(과제 ⑥ 배포)에는 넣지 않으므로 응답이 바뀌지 않는다.

use axum::{
    http::{HeaderName, HeaderValue},
    response::Response,
    Router,
};

/// 인스턴스 이름(예: 파드 이름)을 주는 환경변수.
pub const INSTANCE_NAME_ENV: &str = "STREAMAPP_INSTANCE_NAME";
/// 답한 인스턴스 이름을 싣는 응답 헤더.
pub const INSTANCE_HEADER: &str = "x-streamapp-instance";

/// 환경변수 [`INSTANCE_NAME_ENV`]의 값(앞뒤 공백을 지운 것). 없거나 비어 있으면 `None`.
pub fn instance_name_from_env() -> Option<String> {
    std::env::var(INSTANCE_NAME_ENV)
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// 라우터의 모든 응답에 [`INSTANCE_HEADER`]: `instance_name`을 붙인다. 이름이 없거나 헤더 값으로 쓸 수 없으면
/// 라우터를 그대로 돌려준다.
pub fn tag_responses(router: Router, instance_name: Option<&str>) -> Router {
    let Some(value) = instance_name.and_then(|name| HeaderValue::from_str(name).ok()) else {
        return router;
    };
    router.layer(axum::middleware::map_response(
        move |mut response: Response| {
            let value = value.clone();
            async move {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static(INSTANCE_HEADER), value);
                response
            }
        },
    ))
}

//! Router assembly and middleware.

use std::time::Instant;

use axum::{
    Router,
    extract::{MatchedPath, Request},
    http::{HeaderName, HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{api, openapi, state::AppState};

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub fn router(state: AppState) -> Router {
    let auth = Router::new()
        .route("/config", get(api::auth::config))
        .route("/login", post(api::auth::login))
        .route("/refresh", post(api::auth::refresh_session))
        .route("/logout", post(api::auth::logout))
        .route("/oidc", get(api::auth::oidc_start))
        .route("/oidc/callback", get(api::auth::oidc_callback))
        // Responses carry tokens: never cache them.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    let v1 = Router::new()
        .nest("/auth", auth)
        .route("/me", get(api::me::me))
        .route("/openapi.json", get(openapi_json));

    Router::new()
        .route("/healthz", get(api::health::healthz))
        .route("/readyz", get(api::health::readyz))
        .nest("/api/v1", v1)
        .layer(middleware::from_fn(track_metrics))
        .layer(PropagateRequestIdLayer::new(REQUEST_ID))
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
        .with_state(state)
}

async fn openapi_json() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        openapi::json(),
    )
        .into_response()
}

/// Records `http_requests_total` and `http_request_duration_seconds`.
async fn track_metrics(request: Request, next: Next) -> Response {
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let method = request.method().to_string();
    let start = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16().to_string();
    let labels = [("method", method), ("path", path), ("status", status)];
    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_request_duration_seconds", &labels)
        .record(start.elapsed().as_secs_f64());
    response
}

//! Router assembly and middleware.

use std::time::Instant;

use axum::{
    Router,
    extract::{DefaultBodyLimit, MatchedPath, Request},
    http::{HeaderName, HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{
    api, domain::pictures::PICTURE_MAX_BYTES, openapi, services::images::MAX_UPLOAD_BYTES,
    state::AppState,
};

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

pub fn router(state: AppState) -> Router {
    let auth = Router::new()
        .route("/config", get(api::auth::config))
        .route("/login", post(api::auth::login))
        .route("/refresh", post(api::auth::refresh_session))
        .route("/logout", post(api::auth::logout))
        .route("/oidc", get(api::auth::oidc_start))
        .route("/oidc/callback", get(api::auth::oidc_callback));

    let mut v1 = Router::new()
        .nest("/auth", auth)
        .route("/me", get(api::me::me).patch(api::me::update_me))
        // One byte over the limit, so the handler (not axum) rejects an
        // oversize body with problem JSON.
        .route(
            "/me/picture",
            put(api::me::put_picture)
                .delete(api::me::delete_picture)
                .layer(DefaultBodyLimit::max(PICTURE_MAX_BYTES + 1)),
        )
        .route(
            "/teams",
            get(api::teams::list_teams).post(api::teams::create_team),
        )
        .route(
            "/teams/{team_id}",
            get(api::teams::get_team)
                .patch(api::teams::rename_team)
                .delete(api::teams::delete_team),
        )
        .route("/teams/{team_id}/members", get(api::teams::list_members))
        .route(
            "/teams/{team_id}/members/{user_id}",
            patch(api::teams::update_member).delete(api::teams::remove_member),
        )
        .route(
            "/teams/{team_id}/join-link",
            get(api::teams::get_join_link).post(api::teams::update_join_link),
        )
        .route("/teams/{team_id}/tasks", get(api::tasks::list_tasks))
        // One byte over the limit, so the handler (not axum) rejects an
        // oversize body with problem JSON.
        .route(
            "/teams/{team_id}/attachments",
            post(api::attachments::upload_attachment)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 1)),
        )
        .route(
            "/tasks/{task_id}",
            patch(api::tasks::update_task).delete(api::tasks::delete_task),
        )
        .route("/tasks/{task_id}/position", put(api::tasks::move_task))
        .route(
            "/teams/{team_id}/invitations",
            get(api::invitations::list_invitations).post(api::invitations::create_invitation),
        )
        .route(
            "/teams/{team_id}/invitations/{invitation_id}",
            delete(api::invitations::revoke_invitation),
        )
        .route(
            "/invitations/{token}",
            get(api::invitations::get_invitation),
        )
        .route(
            "/invitations/{token}/accept",
            post(api::invitations::accept_invitation),
        )
        .route(
            "/invitations/{token}/decline",
            post(api::invitations::decline_invitation),
        )
        .route("/join/{token}", post(api::join::join_team))
        .route(
            "/teams/{team_id}/meetings",
            get(api::meetings::list_meetings).post(api::meetings::start_meeting),
        )
        .route(
            "/meetings/{meeting_id}",
            patch(api::meetings::rename_meeting),
        )
        .route(
            "/meetings/{meeting_id}/state",
            get(api::meetings::get_meeting_state),
        )
        .route(
            "/meetings/{meeting_id}/join",
            post(api::meetings::join_meeting),
        )
        .route(
            "/meetings/{meeting_id}/icebreaker/next",
            post(api::meetings::icebreaker_next),
        )
        .route(
            "/meetings/{meeting_id}/icebreaker/goto",
            post(api::meetings::icebreaker_goto),
        )
        .route(
            "/meetings/{meeting_id}/phase/next",
            post(api::meetings::next_phase),
        )
        .route(
            "/meetings/{meeting_id}/facilitator",
            post(api::meetings::set_facilitator),
        )
        .route(
            "/meetings/{meeting_id}/end",
            post(api::meetings::end_meeting),
        )
        .route(
            "/meetings/{meeting_id}/timer",
            put(api::meetings::set_timer),
        )
        .route(
            "/meetings/{meeting_id}/ready",
            put(api::meetings::set_ready),
        )
        .route(
            "/meetings/{meeting_id}/reflections",
            post(api::reflections::create_reflection),
        )
        .route(
            "/reflections/{reflection_id}",
            patch(api::reflections::update_reflection).delete(api::reflections::delete_reflection),
        )
        .route(
            "/meetings/{meeting_id}/invite-link",
            get(api::meetings::get_invite_link).post(api::meetings::regenerate_invite_link),
        )
        .route(
            "/meeting-invites/{token}/accept",
            post(api::meetings::accept_meeting_invite),
        )
        .route("/ws-ticket", post(api::meetings::ws_ticket))
        .route("/ws", get(crate::ws::ws_handler))
        .route("/openapi.json", get(openapi_json));
    // Development-only: refused in production by the configuration.
    if state.config.seed_dev_users {
        v1 = v1.route("/dev/teams/{team_id}/tasks", post(api::dev::seed_task));
    }
    let v1 = v1
        // Responses carry tokens and personal data (access and refresh tokens,
        // invitation and join links): never cache any of them.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    // Picture bytes are immutable per `pictureVersion`, and attachments never
    // change, so these handlers set their own `Cache-Control` on success. Error responses (401, 404) get `no-store`
    // like every other `/api/v1` response.
    let pictures = Router::new()
        .route("/users/{user_id}/picture", get(api::me::get_picture))
        .route(
            "/attachments/{attachment_id}",
            get(api::attachments::get_attachment),
        )
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));
    let v1 = v1.merge(pictures);

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

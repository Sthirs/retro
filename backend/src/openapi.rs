//! OpenAPI document (spec §14), served at `/api/v1/openapi.json` and printed
//! by `retro-backend openapi` for the frontend's generated client.

use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use crate::{
    api,
    domain::timer::TimerStatus,
    entities::{
        meeting::{MeetingPhase, MeetingStatus},
        reflection::ReflectionColumn,
        task::TaskStatus,
    },
    error::Problem,
};

#[derive(OpenApi)]
#[openapi(
    info(title = "Retro API", description = "Team retrospectives."),
    paths(
        api::health::healthz,
        api::health::readyz,
        api::auth::config,
        api::auth::login,
        api::auth::refresh_session,
        api::auth::logout,
        api::auth::oidc_start,
        api::auth::oidc_callback,
        api::me::me,
        api::me::update_me,
        api::me::put_picture,
        api::me::delete_picture,
        api::me::get_picture,
        api::teams::list_teams,
        api::teams::create_team,
        api::teams::get_team,
        api::teams::rename_team,
        api::teams::delete_team,
        api::teams::list_members,
        api::teams::update_member,
        api::teams::remove_member,
        api::teams::get_join_link,
        api::teams::update_join_link,
        api::invitations::create_invitation,
        api::invitations::list_invitations,
        api::invitations::revoke_invitation,
        api::invitations::get_invitation,
        api::invitations::accept_invitation,
        api::invitations::decline_invitation,
        api::join::join_team,
        api::tasks::list_tasks,
        api::tasks::move_task,
        api::tasks::update_task,
        api::tasks::delete_task,
        api::meetings::list_meetings,
        api::meetings::start_meeting,
        api::meetings::get_meeting_state,
        api::meetings::rename_meeting,
        api::meetings::join_meeting,
        api::meetings::icebreaker_next,
        api::meetings::icebreaker_goto,
        api::meetings::next_phase,
        api::meetings::set_facilitator,
        api::meetings::end_meeting,
        api::meetings::set_timer,
        api::meetings::set_ready,
        api::reflections::create_reflection,
        api::reflections::update_reflection,
        api::reflections::delete_reflection,
        api::meetings::get_invite_link,
        api::meetings::regenerate_invite_link,
        api::meetings::accept_meeting_invite,
        api::meetings::ws_ticket,
    ),
    components(schemas(
        Problem,
        TaskStatus,
        MeetingStatus,
        MeetingPhase,
        ReflectionColumn,
        TimerStatus
    )),
    modifiers(&BearerAuth),
)]
pub struct ApiDoc;

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

/// The document as pretty JSON, with a stable version so it does not change
/// with every release.
pub fn json() -> String {
    let mut doc = ApiDoc::openapi();
    doc.info.version = "v1".into();
    doc.to_pretty_json().expect("serializable OpenAPI document")
}

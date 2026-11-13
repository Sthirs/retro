//! OpenAPI document (spec §14), served at `/api/v1/openapi.json` and printed
//! by `retro-backend openapi` for the frontend's generated client.

use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use crate::{api, error::Problem};

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
    ),
    components(schemas(Problem)),
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

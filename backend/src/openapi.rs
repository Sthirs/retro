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

//! `ClientId` extractor: the browser tab behind a request.

use std::convert::Infallible;

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

/// The request header carrying the tab's id.
pub const CLIENT_ID_HEADER: &str = "x-client-id";

/// The id the sending tab picked for itself (`X-Client-Id`), echoed as
/// `clientId` in the real-time events of its changes so that tab can skip its
/// own echo (spec §11.2, ADR-0011). Missing or not a UUID: `None`. Not a
/// credential: it only tells a tab which events it caused.
pub struct ClientId(pub Option<Uuid>);

impl<S: Send + Sync> FromRequestParts<S> for ClientId {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .headers
                .get(CLIENT_ID_HEADER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| Uuid::parse_str(v.trim()).ok()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Request;

    use super::*;

    async fn extract(header: Option<&str>) -> Option<Uuid> {
        let mut request = Request::builder();
        if let Some(value) = header {
            request = request.header(CLIENT_ID_HEADER, value);
        }
        let (mut parts, ()) = request.body(()).unwrap().into_parts();
        let Ok(ClientId(id)) = ClientId::from_request_parts(&mut parts, &()).await;
        id
    }

    #[tokio::test]
    async fn reads_a_uuid_and_ignores_anything_else() {
        let id = Uuid::now_v7();
        assert_eq!(extract(Some(&id.to_string())).await, Some(id));
        assert_eq!(extract(None).await, None);
        assert_eq!(extract(Some("not-a-uuid")).await, None);
        assert_eq!(extract(Some("")).await, None);
    }
}

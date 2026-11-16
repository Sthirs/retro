//! Responses for stored images: user pictures and attachments (spec §20).

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};

use crate::error::AppResult;

/// The response for image bytes that never change under their SHA-256:
/// cached as immutable and revalidated by `ETag`. A request whose
/// `If-None-Match` matches gets `304 Not Modified` without the bytes.
pub(crate) fn immutable_image(
    request: &HeaderMap,
    sha256: &str,
    content_type: HeaderValue,
    data: Vec<u8>,
) -> AppResult<Response> {
    let etag = format!("\"{sha256}\"");
    let not_modified = etag_matches(request, &etag);
    let etag =
        HeaderValue::from_str(&etag).map_err(|e| anyhow::anyhow!("stored image hash: {e}"))?;
    let mut response = if not_modified {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        response
    } else {
        let mut response = Response::new(Body::from(data));
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
        response
    };
    let headers = response.headers_mut();
    headers.insert(header::ETAG, etag);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

/// Whether `If-None-Match` lists `etag` (weak comparison) or is `*`.
fn etag_matches(request: &HeaderMap, etag: &str) -> bool {
    request
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| value.trim() == "*" || entity_tags(value).any(|tag| tag == etag))
}

/// The entity tags of an `If-None-Match` list (RFC 9110 §8.8.3), quotes
/// kept and `W/` dropped. A quoted tag may contain commas, so the list is
/// read tag by tag rather than split; it stops at the first malformed entry
/// (a `*` is only valid as the whole field).
fn entity_tags(list: &str) -> impl Iterator<Item = &str> {
    let mut rest = list;
    std::iter::from_fn(move || {
        rest = rest.trim_start_matches([' ', '\t', ',']);
        let tag = rest.strip_prefix("W/").unwrap_or(rest);
        let end = tag.strip_prefix('"')?.find('"')? + 2;
        let (tag, tail) = tag.split_at(end);
        rest = tail;
        Some(tag)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(value: &str) -> bool {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(value).unwrap());
        etag_matches(&headers, "\"abc\"")
    }

    #[test]
    fn if_none_match_is_compared_weakly_over_lists() {
        assert!(matches("\"abc\""));
        assert!(matches("W/\"abc\""));
        assert!(matches("\"x\", \"abc\""));
        assert!(matches("*"));
        assert!(!matches("\"abcd\""));
        assert!(!matches("abc"));
        assert!(!etag_matches(&HeaderMap::new(), "\"abc\""));
    }

    #[test]
    fn if_none_match_keeps_commas_inside_entity_tags() {
        assert!(!matches("\"x,*,y\""));
        assert!(matches("\"x,*,y\", \"abc\""));
        assert!(matches(" W/\"a,b\" ,W/\"abc\""));
        // `*` is only the whole field, never a list entry.
        assert!(!matches("\"x\", *"));
        assert!(!matches("\"abc"));
    }
}

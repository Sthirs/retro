//! Sanitising of task rich text (a restricted HTML subset) and extraction of
//! the user mentions it contains.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::error::AppError;

/// Maximum length of the sanitized HTML, in characters.
pub const CONTENT_MAX_CHARS: usize = 10_000;

/// Sanitized rich text together with the users it mentions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized {
    pub html: String,
    /// Mentioned user ids, de-duplicated, in order of appearance.
    pub mention_ids: Vec<Uuid>,
}

/// Sanitizes `raw`, rejecting blank or overlong content with
/// `invalid_task_content`.
pub fn sanitize(raw: &str) -> Result<Sanitized, AppError> {
    let cleaned = allowlist().clean(raw).to_string();
    let (html, mention_ids) = normalize_spans(&cleaned);

    if html.chars().count() > CONTENT_MAX_CHARS || visible_text(&html).trim().is_empty() {
        return Err(AppError::Invalid("invalid_task_content"));
    }
    Ok(Sanitized { html, mention_ids })
}

/// The ammonia allowlist: formatting, links and mention spans.
fn allowlist() -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::empty();
    builder
        .tags(HashSet::from(["p", "br", "strong", "em", "a", "span"]))
        .tag_attributes(HashMap::from([
            ("a", HashSet::from(["href"])),
            (
                "span",
                HashSet::from(["data-type", "data-id", "data-label"]),
            ),
        ]))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(Some("noopener noreferrer"));
    builder
}

/// The text a reader sees: every tag removed, entities decoded far enough for
/// a whitespace check.
fn visible_text(html: &str) -> String {
    ammonia::Builder::empty()
        .clean(html)
        .to_string()
        .replace("&nbsp;", "\u{a0}")
}

/// Rewrites every `<span ...>` start tag of ammonia's normalized output.
///
/// ammonia filters attributes one at a time, so it cannot drop a span's
/// attributes when `data-type="mention"` is missing. The output is normalized
/// (double-quoted attributes, escaped values), so a small scan is enough: a
/// mention span keeps exactly `data-type`, a valid canonical `data-id` and
/// `data-label`; any other span keeps no attributes. Returns the new HTML and
/// the de-duplicated mentioned ids in order of appearance.
fn normalize_spans(html: &str) -> (String, Vec<Uuid>) {
    let mut out = String::with_capacity(html.len());
    let mut ids = Vec::new();
    let mut rest = html;

    while let Some(start) = rest.find("<span") {
        let after = &rest[start + "<span".len()..];
        // `<span>` or `<span attr...>`; anything else is not a span tag.
        if !after.starts_with([' ', '>']) {
            out.push_str(&rest[..start + "<span".len()]);
            rest = after;
            continue;
        }
        let Some(end) = tag_end(after) else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&rebuild_span(&after[..end], &mut ids));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    (out, ids)
}

/// Index of the `>` closing a start tag, skipping quoted attribute values.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in tag.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

/// Builds the replacement start tag from the attribute text of a span.
fn rebuild_span(attrs: &str, ids: &mut Vec<Uuid>) -> String {
    let (mut is_mention, mut id, mut label) = (false, None, None);
    for (name, value) in attributes(attrs) {
        match name {
            "data-type" => is_mention = value == "mention",
            "data-id" => id = Uuid::parse_str(value).ok(),
            "data-label" => label = Some(value),
            _ => {}
        }
    }
    if !is_mention {
        return "<span>".into();
    }
    let mut tag = String::from(r#"<span data-type="mention""#);
    if let Some(id) = id {
        if !ids.contains(&id) {
            ids.push(id);
        }
        tag.push_str(&format!(r#" data-id="{id}""#));
    }
    if let Some(label) = label {
        tag.push_str(&format!(r#" data-label="{label}""#));
    }
    tag.push('>');
    tag
}

/// Parses `name="value"` pairs from ammonia-normalized attribute text.
fn attributes(mut attrs: &str) -> Vec<(&str, &str)> {
    let mut pairs = Vec::new();
    while let Some(eq) = attrs.find("=\"") {
        let name = attrs[..eq].trim();
        let value_start = eq + 2;
        let Some(len) = attrs[value_start..].find('"') else {
            break;
        };
        pairs.push((name, &attrs[value_start..value_start + len]));
        attrs = &attrs[value_start + len + 1..];
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "6f1c1d8e-3a52-4d0e-9a53-0c7d6b1f2a10";

    fn invalid(raw: &str) -> bool {
        matches!(
            sanitize(raw),
            Err(AppError::Invalid("invalid_task_content"))
        )
    }

    #[test]
    fn keeps_allowed_markup() {
        let out = sanitize(r#"<p><strong>a</strong> <em>b</em> <a href="https://x.dev">l</a></p>"#)
            .unwrap();
        assert_eq!(
            out.html,
            r#"<p><strong>a</strong> <em>b</em> <a href="https://x.dev" rel="noopener noreferrer">l</a></p>"#
        );
        assert!(out.mention_ids.is_empty());
    }

    #[test]
    fn keeps_line_breaks_and_mailto() {
        let out = sanitize(r#"<p>a<br>b <a href="mailto:a@b.dev">m</a></p>"#).unwrap();
        assert!(out.html.contains("<br>"));
        assert!(out.html.contains(r#"href="mailto:a@b.dev""#));
    }

    #[test]
    fn strips_scripts_and_js_links() {
        let out = sanitize(
            r#"<p style="color:red" onclick="x()">hi</p><script>alert(1)</script><style>p{}</style><img src=x onerror=alert(1)><a href="javascript:alert(1)">bad</a><div>d</div>"#,
        )
        .unwrap();
        assert_eq!(
            out.html,
            r#"<p>hi</p><a rel="noopener noreferrer">bad</a>d"#
        );
    }

    #[test]
    fn extracts_mentions() {
        let raw = format!(
            r#"<p><span data-type="mention" data-id="{ID_A}" data-label="Ana">@Ana</span> and <span data-type="mention" data-id="{ID_A}" data-label="Ana">@Ana</span> <span data-type="mention" data-id="nope" data-label="X">@X</span></p>"#
        );
        let out = sanitize(&raw).unwrap();
        assert_eq!(out.mention_ids, vec![Uuid::parse_str(ID_A).unwrap()]);
        assert!(out.html.contains(&format!(r#"data-id="{ID_A}""#)));
        assert!(!out.html.contains("nope"));
    }

    #[test]
    fn mention_ids_keep_order_of_appearance() {
        let b = "11111111-1111-4111-8111-111111111111";
        let raw = format!(
            r#"<p><span data-type="mention" data-id="{b}">@b</span><span data-type="mention" data-id="{ID_A}">@a</span><span data-type="mention" data-id="{b}">@b</span></p>"#
        );
        let out = sanitize(&raw).unwrap();
        assert_eq!(
            out.mention_ids,
            vec![Uuid::parse_str(b).unwrap(), Uuid::parse_str(ID_A).unwrap()]
        );
    }

    #[test]
    fn span_without_mention_type_keeps_no_attributes() {
        let raw = format!(
            r#"<p><span data-id="{ID_A}" data-label="x" class="c" style="y">t</span><span data-type="other" data-id="{ID_A}">u</span></p>"#
        );
        let out = sanitize(&raw).unwrap();
        assert_eq!(out.html, "<p><span>t</span><span>u</span></p>");
        assert!(out.mention_ids.is_empty());
    }

    #[test]
    fn mention_span_drops_other_attributes() {
        let raw = format!(
            r#"<p><span class="c" onclick="x()" data-type="mention" data-id="{ID_A}" data-label="Ana">@Ana</span></p>"#
        );
        let out = sanitize(&raw).unwrap();
        assert_eq!(
            out.html,
            format!(
                r#"<p><span data-type="mention" data-id="{ID_A}" data-label="Ana">@Ana</span></p>"#
            )
        );
    }

    #[test]
    fn mention_only_paragraph_is_not_blank() {
        let raw = format!(
            r#"<p><span data-type="mention" data-id="{ID_A}" data-label="Ana">@Ana</span></p>"#
        );
        assert!(sanitize(&raw).is_ok());
    }

    #[test]
    fn rejects_blank_and_overlong() {
        assert!(invalid("<p> </p>"));
        assert!(invalid(""));
        assert!(invalid("<p>&nbsp;</p>"));
        assert!(invalid("<script>alert(1)</script>"));
        assert!(invalid(&"a".repeat(CONTENT_MAX_CHARS + 1)));
        assert!(sanitize(&"a".repeat(CONTENT_MAX_CHARS)).is_ok());
    }

    #[test]
    fn length_counts_chars_not_bytes() {
        assert!(sanitize(&"é".repeat(CONTENT_MAX_CHARS)).is_ok());
        assert!(invalid(&"é".repeat(CONTENT_MAX_CHARS + 1)));
    }

    #[test]
    fn span_markup_inside_a_label_is_not_a_mention() {
        let raw = format!(
            r#"<p><span data-type="mention" data-label="a<span data-type=&quot;mention&quot; data-id=&quot;{ID_A}&quot;>b">x</span></p>"#
        );
        let out = sanitize(&raw).unwrap();
        assert!(out.mention_ids.is_empty());
        assert_eq!(out.html.matches("<span").count(), 1);
    }
}

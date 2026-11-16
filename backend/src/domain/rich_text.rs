//! Sanitising of rich text (a restricted HTML subset, ADR-0010) and
//! extraction of the user mentions and image attachments it contains. Tasks
//! and reflections each have their own allow-list ([`Profile`]).
//!
//! Images are `<img data-attachment-id="…" alt="…">` and never carry a `src`
//! (ADR-0013): the client loads them by id with its access token.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::error::AppError;

/// Maximum length of the sanitized HTML, in characters.
pub const CONTENT_MAX_CHARS: usize = 10_000;

/// Maximum number of images in one content.
pub const MAX_IMAGES: usize = 10;

/// Maximum length of the raw input, in characters; longer bodies are rejected
/// without being parsed.
const RAW_MAX_CHARS: usize = 4 * CONTENT_MAX_CHARS;

/// Which content is sanitized: decides the allow-list and the error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Tasks (spec §9.5): formatting, links and mentions.
    Task,
    /// Reflections (spec §9.2): formatting, strikethrough, links, bullet
    /// lists and inline code; no mentions.
    Reflection,
}

impl Profile {
    fn invalid(self) -> AppError {
        AppError::Invalid(match self {
            Self::Task => "invalid_task_content",
            Self::Reflection => "invalid_reflection_content",
        })
    }
}

/// Sanitized rich text together with the users it mentions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized {
    pub html: String,
    /// Mentioned user ids, de-duplicated, in order of appearance.
    pub mention_ids: Vec<Uuid>,
    /// Referenced attachment ids, de-duplicated, in order of appearance.
    pub attachment_ids: Vec<Uuid>,
}

/// Sanitizes task content, rejecting blank or overlong content with
/// `invalid_task_content`.
pub fn sanitize(raw: &str) -> Result<Sanitized, AppError> {
    sanitize_as(raw, Profile::Task)
}

/// Sanitizes `raw` with the allow-list of `profile`, rejecting blank or
/// overlong content with the profile's error code.
pub fn sanitize_as(raw: &str, profile: Profile) -> Result<Sanitized, AppError> {
    // Cheap guard so huge bodies are never parsed. The cap is generous: markup
    // that sanitizes away may still push valid text past `CONTENT_MAX_CHARS`.
    if raw.chars().count() > RAW_MAX_CHARS {
        return Err(profile.invalid());
    }
    let cleaned = allowlist(profile).clean(raw).to_string();
    let (html, mention_ids) = normalize_spans(&cleaned);
    let (html, images) = normalize_imgs(&html);
    let mut attachment_ids = Vec::new();
    for id in &images {
        if !attachment_ids.contains(id) {
            attachment_ids.push(*id);
        }
    }

    if html.chars().count() > CONTENT_MAX_CHARS
        || images.len() > MAX_IMAGES
        || (images.is_empty() && visible_text(&html).trim().is_empty())
    {
        return Err(profile.invalid());
    }
    Ok(Sanitized {
        html,
        mention_ids,
        attachment_ids,
    })
}

/// Adds `width` and `height` to every image of sanitized `html` whose
/// attachment is in `sizes`, so clients can reserve its space before it loads.
pub fn with_image_sizes(html: &str, sizes: &HashMap<Uuid, (i32, i32)>) -> String {
    rewrite_tags(html, "<img", |attrs| {
        let (id, alt) = img_attributes(attrs);
        // Sanitized input: every image has a valid id.
        let Some(id) = id else { return String::new() };
        let mut tag = img_tag(id, alt);
        if let Some((width, height)) = sizes.get(&id) {
            tag.pop();
            tag.push_str(&format!(r#" width="{width}" height="{height}">"#));
        }
        tag
    })
}

/// The ammonia allowlist of a profile.
fn allowlist(profile: Profile) -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::empty();
    let mut attributes = HashMap::from([
        ("a", HashSet::from(["href"])),
        ("img", HashSet::from(["data-attachment-id", "alt"])),
    ]);
    let tags = match profile {
        Profile::Task => {
            attributes.insert(
                "span",
                HashSet::from(["data-type", "data-id", "data-label"]),
            );
            HashSet::from(["p", "br", "strong", "em", "a", "span", "img"])
        }
        Profile::Reflection => HashSet::from([
            "p", "br", "strong", "em", "s", "a", "ul", "li", "code", "img",
        ]),
    };
    builder
        .tags(tags)
        .tag_attributes(attributes)
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        // Only absolute links: `/x`, `//host/x` and `example.com` would resolve
        // against the app's own origin or an attacker's.
        .url_relative(ammonia::UrlRelative::Deny)
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
///
/// Fails closed: a `<span` start tag with no closing `>` is dropped together
/// with everything after it.
fn normalize_spans(html: &str) -> (String, Vec<Uuid>) {
    let mut ids = Vec::new();
    let out = rewrite_tags(html, "<span", |attrs| rebuild_span(attrs, &mut ids));
    (out, ids)
}

/// Rewrites every `<img ...>` tag of ammonia's normalized output to exactly
/// `<img data-attachment-id="…" alt="…">`. An image without a valid
/// canonical attachment id is removed. Returns the new HTML and every
/// referenced id in order of appearance (duplicates included, for the cap).
fn normalize_imgs(html: &str) -> (String, Vec<Uuid>) {
    let mut ids = Vec::new();
    let out = rewrite_tags(html, "<img", |attrs| {
        let (id, alt) = img_attributes(attrs);
        match id {
            Some(id) => {
                ids.push(id);
                img_tag(id, alt)
            }
            None => String::new(),
        }
    });
    (out, ids)
}

/// The attachment id (if canonical) and alt text of an image's attributes.
fn img_attributes(attrs: &str) -> (Option<Uuid>, &str) {
    let (mut id, mut alt) = (None, "");
    for (name, value) in attributes(attrs) {
        match name {
            // Only the canonical hyphenated lowercase form, as the server writes it.
            "data-attachment-id" => {
                id = Uuid::parse_str(value)
                    .ok()
                    .filter(|id| id.hyphenated().to_string() == value);
            }
            "alt" => alt = value,
            _ => {}
        }
    }
    (id, alt)
}

/// The canonical image tag. `alt` is ammonia-escaped already.
fn img_tag(id: Uuid, alt: &str) -> String {
    format!(r#"<img data-attachment-id="{id}" alt="{alt}">"#)
}

/// Replaces every start tag `open` (`"<span"`, `"<img"`) of ammonia's
/// normalized output by `rebuild(attribute text)`.
///
/// Fails closed: a start tag with no closing `>` is dropped together with
/// everything after it.
fn rewrite_tags(html: &str, open: &str, mut rebuild: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        // `<tag>` or `<tag attr...>`; anything else (`<spanx`) is another tag.
        if !after.starts_with([' ', '>', '/']) {
            out.push_str(&rest[..start + open.len()]);
            rest = after;
            continue;
        }
        // Fail closed: a start tag that never closes is dropped together with
        // everything after it, rather than copied through unchecked.
        let Some(end) = tag_end(after) else {
            out.push_str(&rest[..start]);
            rest = "";
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&rebuild(after[..end].trim_end_matches('/')));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
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
    fn removes_relative_and_protocol_relative_links() {
        for href in [
            "/x",
            "//evil.example/x",
            "example.com",
            "x/y",
            "?q=1",
            "#frag",
        ] {
            let out = sanitize(&format!(r#"<p><a href="{href}">t</a></p>"#)).unwrap();
            assert!(!out.html.contains("href"), "{href} kept: {}", out.html);
            assert!(out.html.contains("t</a>"));
        }
    }

    #[test]
    fn keeps_absolute_links() {
        let out =
            sanitize(r#"<p><a href="https://x.dev/p?q=1">a</a><a href="mailto:a@b.dev">b</a></p>"#)
                .unwrap();
        assert!(out.html.contains(r#"href="https://x.dev/p?q=1""#));
        assert!(out.html.contains(r#"href="mailto:a@b.dev""#));
    }

    #[test]
    fn span_markup_inside_an_href_injects_nothing() {
        let out = sanitize(
            r#"<p><a href="https://x/<span data-type=&quot;mention&quot; data-id=&quot;6f1c1d8e-3a52-4d0e-9a53-0c7d6b1f2a10&quot;>">t</a></p>"#,
        )
        .unwrap();
        // ammonia escapes the `<` inside the attribute, so the scan never sees
        // a span start tag there: no element, no unescaped attributes.
        assert!(out.mention_ids.is_empty());
        assert!(!out.html.contains("<span"));
        assert!(!out.html.contains(r#"data-type=""#));
        assert!(!out.html.contains(r#"data-id=""#));
        assert_eq!(out.html.matches("<a ").count(), 1);
    }

    #[test]
    fn unterminated_span_start_tag_is_dropped() {
        let (html, ids) = normalize_spans(r#"<p>a</p><span data-type="mention" data-id="x"#);
        assert_eq!(html, "<p>a</p>");
        assert!(ids.is_empty());
    }

    #[test]
    fn rejects_raw_input_beyond_the_cheap_cap() {
        assert!(invalid(&format!("<p>x</p>{}", " ".repeat(RAW_MAX_CHARS))));
        assert!(invalid(&"<p></p>".repeat(RAW_MAX_CHARS / 7 + 1)));
        // Markup that sanitizes away still counts below the cap.
        assert!(sanitize(&format!("<p>x</p><script>{}</script>", "a".repeat(20_000))).is_ok());
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

    #[test]
    fn reflections_keep_their_extra_formatting() {
        let raw = "<p><s>old</s> <code>x()</code></p><ul><li><p>one</p></li></ul>";
        let out = sanitize_as(raw, Profile::Reflection).unwrap();
        assert_eq!(out.html, raw);
        // Tasks do not allow it.
        let task = sanitize(raw).unwrap();
        assert!(!task.html.contains("<s>") && !task.html.contains("<ul>"));
    }

    #[test]
    fn reflections_drop_mentions_scripts_and_images() {
        let id = Uuid::now_v7();
        let raw = format!(
            r#"<p>hi <span data-type="mention" data-id="{id}" data-label="Bo">@Bo</span><img src="x" onerror="alert(1)"><script>alert(1)</script></p>"#
        );
        let out = sanitize_as(&raw, Profile::Reflection).unwrap();
        assert_eq!(out.html, "<p>hi @Bo</p>");
        assert!(out.mention_ids.is_empty());
    }

    #[test]
    fn reflections_have_their_own_error_code() {
        assert!(matches!(
            sanitize_as("<p> </p>", Profile::Reflection),
            Err(AppError::Invalid("invalid_reflection_content"))
        ));
        assert!(matches!(
            sanitize_as(&"a".repeat(CONTENT_MAX_CHARS + 1), Profile::Reflection),
            Err(AppError::Invalid("invalid_reflection_content"))
        ));
    }

    const IMG: &str = "0193a4b2-7c1d-7e3f-8a9b-0c1d2e3f4a5b";

    #[test]
    fn images_keep_only_their_attachment_id_and_alt() {
        let raw = format!(
            r#"<p>a<img src="https://evil.example/t.gif" data-attachment-id="{IMG}" alt="a &quot;cat&quot;" onerror="alert(1)" style="x" width="9999" class="c"></p>"#
        );
        for profile in [Profile::Task, Profile::Reflection] {
            let out = sanitize_as(&raw, profile).unwrap();
            assert_eq!(
                out.html,
                format!(r#"<p>a<img data-attachment-id="{IMG}" alt="a &quot;cat&quot;"></p>"#)
            );
            assert_eq!(out.attachment_ids, vec![Uuid::parse_str(IMG).unwrap()]);
        }
    }

    #[test]
    fn images_without_a_canonical_id_are_removed() {
        for img in [
            r#"<img src="x" onerror="alert(1)">"#.to_owned(),
            r#"<img data-attachment-id="nope">"#.to_owned(),
            format!(r#"<img data-attachment-id="{{{IMG}}}">"#),
            format!(r#"<img data-attachment-id="{}">"#, IMG.to_uppercase()),
            format!(r#"<img data-attachment-id="urn:uuid:{IMG}">"#),
            r#"<img src="javascript:alert(1)">"#.to_owned(),
            r#"<img src="data:image/svg+xml,<svg onload=alert(1)>">"#.to_owned(),
        ] {
            let out = sanitize(&format!("<p>t{img}</p>")).unwrap();
            assert_eq!(out.html, "<p>t</p>", "{img}");
            assert!(out.attachment_ids.is_empty());
        }
    }

    #[test]
    fn image_markup_in_text_or_attributes_is_not_an_image() {
        let raw = format!(
            r#"<p>&lt;img data-attachment-id="{IMG}"&gt; <a href="https://x/<img data-attachment-id=&quot;{IMG}&quot;>">l</a></p>"#
        );
        let out = sanitize(&raw).unwrap();
        assert!(out.attachment_ids.is_empty());
        assert!(!out.html.contains("<img"));
    }

    #[test]
    fn image_only_content_is_not_blank() {
        let out = sanitize(&format!(r#"<p><img data-attachment-id="{IMG}"></p>"#)).unwrap();
        assert_eq!(
            out.html,
            format!(r#"<p><img data-attachment-id="{IMG}" alt=""></p>"#)
        );
        assert!(sanitize("<p><img src=\"x\"></p>").is_err());
    }

    #[test]
    fn image_ids_are_deduplicated_and_capped() {
        let one = format!(r#"<img data-attachment-id="{IMG}">"#);
        let out = sanitize(&format!("<p>{one}{one}</p>")).unwrap();
        assert_eq!(out.attachment_ids.len(), 1);
        assert!(sanitize(&format!("<p>{}</p>", one.repeat(MAX_IMAGES))).is_ok());
        assert!(invalid(&format!("<p>{}</p>", one.repeat(MAX_IMAGES + 1))));
    }

    #[test]
    fn image_sizes_are_added_from_the_attachments() {
        let other = Uuid::now_v7();
        let html = sanitize(&format!(
            r#"<p><img data-attachment-id="{IMG}" alt="x"><img data-attachment-id="{other}"></p>"#
        ))
        .unwrap()
        .html;
        let sizes = HashMap::from([(Uuid::parse_str(IMG).unwrap(), (640, 480))]);
        assert_eq!(
            with_image_sizes(&html, &sizes),
            format!(
                r#"<p><img data-attachment-id="{IMG}" alt="x" width="640" height="480"><img data-attachment-id="{other}" alt=""></p>"#
            )
        );
        // Sizing again replaces, never duplicates, the dimensions.
        let twice = with_image_sizes(&with_image_sizes(&html, &sizes), &sizes);
        assert_eq!(twice, with_image_sizes(&html, &sizes));
    }
}

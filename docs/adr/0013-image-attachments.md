# ADR-0013: Image attachments are re-encoded as WebP and referenced by id in rich text

- **Status:** 🟡 proposed
- **Date:** 2026-11-16
- **Deciders:** Retro maintainers

## Context

Reflections (spec §9.2) and tasks (spec §9.5, §10.1) carry images, static or animated
(GIFs). They are uploaded to the backend and stored in PostgreSQL (spec §2, §13).
[ADR-0010](0010-rich-text-format.md) stores rich text as server-sanitized HTML and lists
images as a follow-up for its allow-list.

Images are a classic attack vector:

- markup that loads or runs something (`<img onerror>`, `javascript:` and remote URLs, SVG);
- polyglot files (a valid GIF with HTML or script appended);
- metadata that leaks personal data (EXIF GPS);
- decompression bombs;
- memory-safety bugs in image parsers, such as libwebp's CVE-2023-4863.

Attachments must also respect Reflect privacy (spec §9.2) and reflection anonymity.

## Decision

We will decode every upload with **memory-safe Rust decoders** (`image`, `gif`,
`image-webp`) under explicit size, dimension, frame and allocation limits, then
**re-encode the pixels as WebP** with libwebp (bundled through `webp-animation`). Rich text
references the result by attachment id only, as
`<img data-attachment-id="…" alt="…" width="…" height="…">`, never by URL.

- **Decoding.** Untrusted bytes are never given to C code. libwebp only ever encodes pixel
  buffers the backend produced.
- **What survives.** Only pixels and frame timing survive. Metadata, extension blocks and
  trailing bytes are dropped, and a JPEG's EXIF orientation is applied before encoding.
- **Encoding.** Animated GIF and WebP stay animated. JPEG and lossy WebP become lossy
  WebP. PNG, GIF (at most 256 colours per frame) and lossless WebP become lossless WebP.
- **Sanitizing.** The server and client sanitizers allow `img` with those four attributes
  and never `src`. The client turns ids into `blob:` URLs after an authenticated fetch (spec
  §15.4), so the CSP (`img-src 'self' data: blob:`) is unchanged.
- **Read access.** An image is served to team members who uploaded it or can see an item
  that references it.
- **Write access.** An item may only reference attachments of its team that it already
  referenced or that the saving user uploaded. This stops a user from copying someone
  else's unrevealed reflection image into a task.

## Alternatives considered

- **Store uploads as received** (as profile pictures are). Validating magic bytes does not
  remove polyglot payloads or GPS metadata, and every client parses arbitrary files.
- **Re-encode to the source format.** Animated GIFs stay large, and the backend would need
  four encoders instead of one.
- **Pure-Rust WebP encoding (`image-webp`).** It only encodes lossless stills and has no
  animation encoder.
- **Decode with libwebp too.** It is simpler, but it puts untrusted bytes through C code
  with a history of exploitable bugs.
- **External hosting or a GIF search (Giphy, Tenor).** It leaks users' IPs and queries to a
  third party, needs the CSP widened, and allows tracking pixels.
- **`src` URLs in the stored HTML.** `<img src>` cannot carry the bearer token (ADR-0005),
  and a stored URL can be changed to point anywhere.

## Sources / Prior art

- Spec §2, §9.2, §9.5, §10.1, §13, §14.4, §15.4 and §20.
- [ADR-0005](0005-authentication.md) (bearer tokens) and
  [ADR-0010](0010-rich-text-format.md) (sanitized HTML).
- OWASP File Upload Cheat Sheet, which recommends rewriting images ("content disarm and
  reconstruction"): <https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html>.
- CVE-2023-4863, a heap overflow in libwebp's lossless decoder:
  <https://nvd.nist.gov/vuln/detail/CVE-2023-4863>.
- libwebp `WebPAnimEncoder` API: <https://developers.google.com/speed/webp/docs/container-api>.
- `image` crate decoding limits: <https://docs.rs/image/latest/image/struct.Limits.html>.
- `webp-animation`: <https://docs.rs/webp-animation>.

## Consequences

- Positive: stored bytes are always produced by the backend, so payloads hidden in uploads
  cannot reach a browser. Clients only ever receive `image/webp`, and animated GIFs usually
  get smaller.
- Positive: no new infrastructure. PostgreSQL stays the only stateful dependency.
- Negative: the build compiles a C library (libwebp). It needs a C compiler, which the Rust
  build image already has, and Renovate has to track it.
- Negative: re-encoding costs CPU on upload. It is bounded by the decoding limits and a
  small concurrency limit.
- Negative: lossy re-encoding of JPEGs and lossy WebPs slightly lowers their quality. APNG
  keeps only its first frame.
- Follow-ups: a lightbox to view images at full size; pausing animations for users who
  prefer reduced motion.

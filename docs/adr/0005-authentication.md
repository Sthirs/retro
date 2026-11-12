# ADR-0005: Backend-driven authentication: local accounts and OIDC

- **Status:** 🟡 proposed
- **Date:** 2026-11-12
- **Deciders:** Retro maintainers

## Context

The Sign in design offers a username/password form and a "Continue with single
sign-on" button. A frontend-only OIDC public client cannot implement the password
form, and keeping provider tokens in the browser exposes them to scripts. We need
one session model for both sign-in methods.

## Decision

We will authenticate in the backend and issue Retro's own session:

- **Local accounts**: users may have an Argon2id password hash. `POST /auth/login`
  accepts a username or email. Unknown users are verified against a dummy hash so
  timing does not reveal which accounts exist. Failed attempts are rate-limited per
  login name and per client IP (in memory, per replica).
- **Single sign-on**: the backend is an OIDC **confidential client** using the
  Authorization Code flow with **PKCE**, state and nonce, stored in an encrypted,
  10-minute, `SameSite=Lax` cookie. The ID token must carry `email_verified=true`.
  Provider metadata and keys are discovered lazily and re-fetched once when ID token
  verification fails (key rotation).
- **Account resolution** for SSO, in one transaction: a known `(issuer, subject)` →
  that user; else a user with the same verified email gets the identity linked; else a
  new user is created when `OIDC_AUTO_REGISTER` is on. Name, email and picture are
  synced from the provider on each sign-in.
- **Session**: a short-lived HS256 access JWT (default 15 min), held in memory by the
  frontend and sent as `Authorization: Bearer`; and an opaque, rotating, single-use
  **refresh token** in an httpOnly `SameSite=Strict` cookie scoped to `/api/v1/auth`.
  Only its SHA-256 is stored. Reusing a rotated token revokes the whole token family,
  except within a short grace window for concurrent tabs. "Keep me signed in" makes
  the cookie persistent (30 days); otherwise it is a browser-session cookie (12 hours
  server-side).
- **No token in URLs**: the SSO callback sets the refresh cookie and redirects to
  `/auth/callback`; the frontend exchanges the cookie for an access token.

## Alternatives considered

- **Frontend OIDC public client (oidc-client-ts)** — cannot serve the password form;
  provider tokens would live in the browser.
- **Server-side sessions only (cookie on every request)** — needs CSRF protection on
  every mutating endpoint and does not fit the WebSocket ticket flow as well.
- **Provider password grant (ROPC)** — deprecated by OAuth 2.1 and unsupported by most
  providers.
- **Never link by email** — users who first signed in with a password would get a
  second account on their first SSO sign-in. Linking requires a verified email, so it
  is only as strong as the provider's email verification: configure only providers you
  trust for that.

## Sources / Prior art

- Spec §3, §14.1, §20.
- OAuth 2.0 for Browser-Based Apps (BFF pattern):
  <https://datatracker.ietf.org/doc/html/draft-ietf-oauth-browser-based-apps>.
- OAuth 2.0 Security Best Current Practice (RFC 9700), refresh token rotation:
  <https://www.rfc-editor.org/rfc/rfc9700>.
- OWASP Password Storage Cheat Sheet (Argon2id):
  <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html>.

## Consequences

- Positive: one session model for both methods; provider tokens never reach the browser.
- Positive: tokens are short-lived and refresh-token theft is detectable.
- Negative: Retro stores password hashes and must protect them; password reset is
  delegated (`AUTH_PASSWORD_RESET_URL`) because the application sends no email.
- Negative: rate limiting is per replica.
- Follow-ups: WebSocket authentication via short-lived tickets (spec §3); provider
  logout (`end_session_endpoint`); account management for local users.

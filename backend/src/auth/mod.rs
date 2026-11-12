//! Authentication: local passwords, SSO through OIDC, Retro-issued sessions
//! (spec §3, ADR-0005).

pub mod cookies;
pub mod extractor;
pub mod jwt;
pub mod oidc;
pub mod password;
pub mod rate_limit;
pub mod refresh;

pub use extractor::CurrentUser;

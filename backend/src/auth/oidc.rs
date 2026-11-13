//! SSO through an OpenID Connect provider. The backend is a confidential
//! client using the Authorization Code flow with PKCE (spec §3, ADR-0005).

use std::sync::Arc;

use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use url::Url;

use crate::{config::OidcConfig, error::AppError};

type Client = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// What the browser carries (encrypted) between the redirect and the callback.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginState {
    pub csrf: String,
    pub nonce: String,
    pub pkce_verifier: String,
    /// Where to land after sign-in; always a local path.
    pub redirect: String,
}

/// The verified identity returned by the provider.
#[derive(Debug, Clone)]
pub struct Identity {
    pub issuer: String,
    pub subject: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
}

pub struct OidcProvider {
    config: OidcConfig,
    redirect_url: Url,
    http: reqwest::Client,
    client: RwLock<Option<Arc<Client>>>,
}

impl OidcProvider {
    pub fn new(config: OidcConfig, redirect_url: Url) -> Self {
        let http = reqwest::ClientBuilder::new()
            // Following redirects opens the client up to SSRF.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("http client");
        Self {
            config,
            redirect_url,
            http,
            client: RwLock::new(None),
        }
    }

    pub fn button_text(&self) -> &str {
        &self.config.button_text
    }

    pub fn auto_register(&self) -> bool {
        self.config.auto_register
    }

    /// Discovers the provider on first use and caches the client. A failed
    /// discovery is retried on the next request instead of failing startup.
    async fn client(&self) -> Result<Arc<Client>, AppError> {
        if let Some(client) = self.client.read().await.as_ref() {
            return Ok(client.clone());
        }
        let mut slot = self.client.write().await;
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let issuer = IssuerUrl::new(
            self.config
                .issuer_url
                .as_str()
                .trim_end_matches('/')
                .to_owned(),
        )
        .map_err(|e| AppError::Unavailable(e.to_string()))?;
        let metadata = CoreProviderMetadata::discover_async(issuer, &self.http)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "OIDC discovery failed");
                AppError::Unavailable("identity provider unreachable".into())
            })?;
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::from_url(self.redirect_url.clone()));
        let client = Arc::new(client);
        *slot = Some(client.clone());
        Ok(client)
    }

    /// Builds the provider's authorization URL and the state to remember.
    pub async fn authorize(&self, redirect: String) -> Result<(Url, LoginState), AppError> {
        let client = self.client().await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for scope in &self.config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let (url, csrf, nonce) = request.set_pkce_challenge(challenge).url();
        Ok((
            url,
            LoginState {
                csrf: csrf.secret().clone(),
                nonce: nonce.secret().clone(),
                pkce_verifier: verifier.secret().clone(),
                redirect,
            },
        ))
    }

    /// Exchanges the code and verifies the ID token. Requires a verified email.
    pub async fn complete(
        &self,
        state: &LoginState,
        returned_state: &str,
        code: String,
    ) -> Result<Identity, anyhow::Error> {
        anyhow::ensure!(
            constant_time_eq(state.csrf.as_bytes(), returned_state.as_bytes()),
            "state mismatch"
        );
        let client = self.client().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let response = client
            .exchange_code(AuthorizationCode::new(code))?
            .set_pkce_verifier(PkceCodeVerifier::new(state.pkce_verifier.clone()))
            .request_async(&self.http)
            .await?;
        let id_token = response
            .id_token()
            .ok_or_else(|| anyhow::anyhow!("provider returned no ID token"))?;
        let nonce = Nonce::new(state.nonce.clone());
        let claims = match id_token.claims(&client.id_token_verifier(), &nonce) {
            Ok(claims) => claims.clone(),
            // The provider may have rotated its signing keys since discovery:
            // refresh the cached metadata and JWKS once, then verify again.
            Err(first) => {
                tracing::info!(error = %first, "ID token verification failed; refreshing provider keys");
                *self.client.write().await = None;
                let fresh = self.client().await.map_err(|e| anyhow::anyhow!("{e}"))?;
                id_token.claims(&fresh.id_token_verifier(), &nonce)?.clone()
            }
        };

        let email = claims
            .email()
            .map(|e| e.as_str().to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("ID token has no email claim"))?;
        anyhow::ensure!(claims.email_verified() == Some(true), "email not verified");

        Ok(Identity {
            issuer: claims.issuer().as_str().to_owned(),
            subject: claims.subject().as_str().to_owned(),
            email,
            name: claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| n.as_str().to_owned()),
            picture: claims
                .picture()
                .and_then(|p| p.get(None))
                .map(|p| p.as_str().to_owned()),
        })
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Accepts only local absolute paths (`/teams/…`), never `//host` or URLs.
pub fn sanitize_redirect(target: Option<&str>) -> String {
    match target {
        Some(t) if t.starts_with('/') && !t.starts_with("//") && !t.contains('\\') => t.to_owned(),
        _ => "/".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_redirects() {
        assert_eq!(sanitize_redirect(Some("/teams/1")), "/teams/1");
        assert_eq!(sanitize_redirect(Some("//evil.example")), "/");
        assert_eq!(sanitize_redirect(Some("https://evil.example")), "/");
        assert_eq!(sanitize_redirect(Some("/\\evil.example")), "/");
        assert_eq!(sanitize_redirect(None), "/");
    }

    #[test]
    fn compares_in_constant_time() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abx"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}

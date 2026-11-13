//! Runtime configuration, read from environment variables (spec §16.1).
//!
//! Every variable is documented in `docs/development.md` ("Configuration").

use std::net::SocketAddr;

use openidconnect::reqwest::Certificate;
use url::Url;

/// Source of configuration variables (the process environment, or a map in tests).
pub trait Vars {
    fn var(&self, name: &str) -> Option<String>;
}

/// Reads the process environment.
pub struct ProcessEnv;

impl Vars for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

impl Vars for std::collections::HashMap<&str, &str> {
    fn var(&self, name: &str) -> Option<String> {
        self.get(name).map(|v| (*v).to_owned())
    }
}

struct Reader<'a, V: Vars>(&'a V);

impl<V: Vars> Reader<'_, V> {
    fn str(&self, name: &'static str) -> Option<String> {
        self.0.var(name).filter(|v| !v.trim().is_empty())
    }

    fn parse<T: std::str::FromStr>(&self, name: &'static str) -> Result<Option<T>, ConfigError>
    where
        T::Err: std::fmt::Display,
    {
        self.str(name)
            .map(|v| {
                v.trim().parse::<T>().map_err(|e| ConfigError::Invalid {
                    name,
                    reason: e.to_string(),
                })
            })
            .transpose()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnv {
    Development,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMigrations {
    Startup,
    None,
}

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer_url: Url,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    pub button_text: String,
    pub auto_register: bool,
    /// Extra root CAs trusted for calls to the provider, besides the public
    /// web PKI (`OIDC_CA_CERT_FILE`, e.g. a private CA or the dev mkcert CA).
    pub ca_certs: Vec<Certificate>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub app_env: AppEnv,
    pub bind_addr: SocketAddr,
    pub metrics_addr: SocketAddr,
    /// External base URL of the application (scheme + host), without trailing slash.
    pub public_url: Url,
    pub database_url: String,
    pub run_migrations: RunMigrations,
    pub jwt_secret: String,
    pub state_secret: String,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_seconds: i64,
    pub session_refresh_ttl_seconds: i64,
    pub refresh_reuse_grace_seconds: i64,
    /// Failed sign-in attempts allowed per login name and per client IP in 5 minutes.
    pub login_rate_limit: u32,
    pub local_login_enabled: bool,
    pub password_reset_url: Option<Url>,
    pub oidc: Option<OidcConfig>,
    pub seed_dev_users: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("{name} is invalid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

const MIN_SECRET_LEN: usize = 32;

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&ProcessEnv)
    }

    pub fn from_vars(vars: &impl Vars) -> Result<Self, ConfigError> {
        let r = Reader(vars);
        let app_env = match r.str("APP_ENV").as_deref().unwrap_or("production") {
            "production" => AppEnv::Production,
            "development" => AppEnv::Development,
            other => {
                return Err(ConfigError::Invalid {
                    name: "APP_ENV",
                    reason: format!("expected development|production, got {other:?}"),
                });
            }
        };

        let public_url = parse_url(
            "PUBLIC_URL",
            &r.str("PUBLIC_URL")
                .ok_or(ConfigError::Missing("PUBLIC_URL"))?,
        )?;

        let database_url = match r.str("DATABASE_URL") {
            Some(url) => url,
            None => {
                let host = r
                    .str("DB_HOST")
                    .ok_or(ConfigError::Missing("DATABASE_URL or DB_HOST"))?;
                let user = r.str("DB_USER").ok_or(ConfigError::Missing("DB_USER"))?;
                let password = r.str("DB_PASSWORD").unwrap_or_default();
                let name = r.str("DB_NAME").unwrap_or_else(|| "retro".into());
                let port = r.parse::<u16>("DB_PORT")?.unwrap_or(5432);
                let sslmode = r.str("DB_SSLMODE").unwrap_or_else(|| "prefer".into());
                let mut url = Url::parse("postgres://localhost").expect("static url");
                url.set_host(Some(&host))
                    .map_err(|e| ConfigError::Invalid {
                        name: "DB_HOST",
                        reason: e.to_string(),
                    })?;
                let _ = url.set_port(Some(port));
                let _ = url.set_username(&user);
                if !password.is_empty() {
                    let _ = url.set_password(Some(&password));
                }
                url.set_path(&name);
                url.query_pairs_mut().append_pair("sslmode", &sslmode);
                url.to_string()
            }
        };

        let run_migrations = match r.str("RUN_MIGRATIONS").as_deref().unwrap_or("startup") {
            "startup" => RunMigrations::Startup,
            "none" | "job" => RunMigrations::None,
            other => {
                return Err(ConfigError::Invalid {
                    name: "RUN_MIGRATIONS",
                    reason: format!("expected startup|job|none, got {other:?}"),
                });
            }
        };

        let jwt_secret = secret("JWT_SECRET", r.str("JWT_SECRET"))?;
        let state_secret = secret("STATE_SECRET", r.str("STATE_SECRET"))?;

        let oidc = if r.parse::<bool>("OIDC_ENABLED")?.unwrap_or(false) {
            Some(OidcConfig {
                issuer_url: parse_url(
                    "OIDC_ISSUER_URL",
                    &r.str("OIDC_ISSUER_URL")
                        .ok_or(ConfigError::Missing("OIDC_ISSUER_URL"))?,
                )?,
                client_id: r
                    .str("OIDC_CLIENT_ID")
                    .ok_or(ConfigError::Missing("OIDC_CLIENT_ID"))?,
                client_secret: r
                    .str("OIDC_CLIENT_SECRET")
                    .ok_or(ConfigError::Missing("OIDC_CLIENT_SECRET"))?,
                scopes: r
                    .str("OIDC_SCOPES")
                    .unwrap_or_else(|| "openid email profile".into())
                    .split_whitespace()
                    .filter(|s| *s != "openid")
                    .map(str::to_owned)
                    .collect(),
                button_text: r
                    .str("OIDC_BUTTON_TEXT")
                    .unwrap_or_else(|| "Continue with single sign-on".into()),
                auto_register: r.parse::<bool>("OIDC_AUTO_REGISTER")?.unwrap_or(true),
                ca_certs: r
                    .str("OIDC_CA_CERT_FILE")
                    .map(|path| ca_certs("OIDC_CA_CERT_FILE", &path))
                    .transpose()?
                    .unwrap_or_default(),
            })
        } else {
            None
        };

        let seed_dev_users = r.parse::<bool>("SEED_DEV_USERS")?.unwrap_or(false);
        if seed_dev_users && app_env == AppEnv::Production {
            return Err(ConfigError::Invalid {
                name: "SEED_DEV_USERS",
                reason: "dev users cannot be seeded when APP_ENV=production".into(),
            });
        }

        Ok(Self {
            app_env,
            bind_addr: parse_addr(
                "BIND_ADDR",
                r.str("BIND_ADDR").as_deref().unwrap_or("0.0.0.0:3000"),
            )?,
            metrics_addr: parse_addr(
                "METRICS_ADDR",
                r.str("METRICS_ADDR").as_deref().unwrap_or("0.0.0.0:9090"),
            )?,
            public_url,
            database_url,
            run_migrations,
            jwt_secret,
            state_secret,
            access_token_ttl_seconds: r.parse("ACCESS_TOKEN_TTL_SECONDS")?.unwrap_or(900),
            refresh_token_ttl_seconds: r
                .parse("REFRESH_TOKEN_TTL_SECONDS")?
                .unwrap_or(30 * 24 * 3600),
            session_refresh_ttl_seconds: r
                .parse("SESSION_REFRESH_TTL_SECONDS")?
                .unwrap_or(12 * 3600),
            refresh_reuse_grace_seconds: r.parse("REFRESH_REUSE_GRACE_SECONDS")?.unwrap_or(10),
            login_rate_limit: r.parse("LOGIN_RATE_LIMIT")?.unwrap_or(10),
            local_login_enabled: r.parse("AUTH_LOCAL_LOGIN_ENABLED")?.unwrap_or(true),
            password_reset_url: r
                .str("AUTH_PASSWORD_RESET_URL")
                .map(|s| parse_url("AUTH_PASSWORD_RESET_URL", &s))
                .transpose()?,
            oidc,
            seed_dev_users,
        })
    }

    /// Absolute URL of a path under [`Config::public_url`].
    pub fn public(&self, path: &str) -> Url {
        self.public_url.join(path).expect("valid path")
    }

    /// A configuration for tests: local login on, SSO off, short-lived secrets.
    pub fn for_tests(database_url: &str) -> Self {
        Self {
            app_env: AppEnv::Development,
            bind_addr: "127.0.0.1:0".parse().expect("addr"),
            metrics_addr: "127.0.0.1:0".parse().expect("addr"),
            public_url: Url::parse("http://retro.test").expect("url"),
            database_url: database_url.to_owned(),
            run_migrations: RunMigrations::Startup,
            jwt_secret: "test-jwt-secret-test-jwt-secret-0000".into(),
            state_secret: "test-state-secret-test-state-secret-0000".into(),
            access_token_ttl_seconds: 900,
            refresh_token_ttl_seconds: 30 * 24 * 3600,
            session_refresh_ttl_seconds: 12 * 3600,
            refresh_reuse_grace_seconds: 0,
            login_rate_limit: 1000,
            local_login_enabled: true,
            password_reset_url: None,
            oidc: None,
            seed_dev_users: true,
        }
    }
}

fn secret(name: &'static str, value: Option<String>) -> Result<String, ConfigError> {
    let value = value.ok_or(ConfigError::Missing(name))?;
    if value.len() < MIN_SECRET_LEN {
        return Err(ConfigError::Invalid {
            name,
            reason: format!("must be at least {MIN_SECRET_LEN} characters"),
        });
    }
    Ok(value)
}

/// Reads a PEM bundle of one or more CA certificates.
fn ca_certs(name: &'static str, path: &str) -> Result<Vec<Certificate>, ConfigError> {
    let invalid = |reason: String| ConfigError::Invalid { name, reason };
    let pem = std::fs::read(path).map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
    let certs = Certificate::from_pem_bundle(&pem).map_err(|e| invalid(e.to_string()))?;
    if certs.is_empty() {
        return Err(invalid(format!("no PEM certificate in {path}")));
    }
    Ok(certs)
}

fn parse_url(name: &'static str, value: &str) -> Result<Url, ConfigError> {
    let url = Url::parse(value).map_err(|e| ConfigError::Invalid {
        name,
        reason: e.to_string(),
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ConfigError::Invalid {
            name,
            reason: "must be an http(s) URL".into(),
        });
    }
    Ok(url)
}

fn parse_addr(name: &'static str, value: &str) -> Result<SocketAddr, ConfigError> {
    value
        .parse()
        .map_err(|e: std::net::AddrParseError| ConfigError::Invalid {
            name,
            reason: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn base() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("PUBLIC_URL", "http://retro.local"),
            ("DB_HOST", "db"),
            ("DB_USER", "retro"),
            ("DB_PASSWORD", "p@ss word"),
            ("DB_SSLMODE", "require"),
            ("JWT_SECRET", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
            ("STATE_SECRET", "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"),
        ])
    }

    #[test]
    fn builds_database_url_from_parts() {
        let cfg = Config::from_vars(&base()).unwrap();
        assert_eq!(
            cfg.database_url,
            "postgres://retro:p%40ss%20word@db:5432/retro?sslmode=require"
        );
        assert_eq!(cfg.app_env, AppEnv::Production);
        assert!(cfg.oidc.is_none());
    }

    #[test]
    fn database_url_wins_over_parts() {
        let mut vars = base();
        vars.insert("DATABASE_URL", "postgres://a@b/c");
        assert_eq!(
            Config::from_vars(&vars).unwrap().database_url,
            "postgres://a@b/c"
        );
    }

    #[test]
    fn rejects_short_secrets() {
        let mut vars = base();
        vars.insert("JWT_SECRET", "short");
        assert!(matches!(
            Config::from_vars(&vars),
            Err(ConfigError::Invalid {
                name: "JWT_SECRET",
                ..
            })
        ));
    }

    #[test]
    fn rejects_invalid_booleans() {
        let mut vars = base();
        vars.insert("AUTH_LOCAL_LOGIN_ENABLED", "maybe");
        assert!(matches!(
            Config::from_vars(&vars),
            Err(ConfigError::Invalid {
                name: "AUTH_LOCAL_LOGIN_ENABLED",
                ..
            })
        ));
    }

    #[test]
    fn refuses_dev_seed_in_production() {
        let mut vars = base();
        vars.insert("SEED_DEV_USERS", "true");
        assert!(Config::from_vars(&vars).is_err());
        vars.insert("APP_ENV", "development");
        assert!(Config::from_vars(&vars).unwrap().seed_dev_users);
    }

    #[test]
    fn requires_oidc_settings_when_enabled() {
        let mut vars = base();
        vars.insert("OIDC_ENABLED", "true");
        assert!(matches!(
            Config::from_vars(&vars),
            Err(ConfigError::Missing("OIDC_ISSUER_URL"))
        ));
        vars.insert("OIDC_ISSUER_URL", "http://retro.local/dex");
        vars.insert("OIDC_CLIENT_ID", "retro");
        vars.insert("OIDC_CLIENT_SECRET", "s");
        let oidc = Config::from_vars(&vars).unwrap().oidc.unwrap();
        assert_eq!(oidc.scopes, vec!["email", "profile"]);
        assert!(oidc.auto_register);
    }

    #[test]
    fn loads_extra_oidc_ca_certificates() {
        let mut vars = base();
        vars.insert("OIDC_ENABLED", "true");
        vars.insert("OIDC_ISSUER_URL", "https://retro.local/dex");
        vars.insert("OIDC_CLIENT_ID", "retro");
        vars.insert("OIDC_CLIENT_SECRET", "s");
        assert!(
            Config::from_vars(&vars)
                .unwrap()
                .oidc
                .unwrap()
                .ca_certs
                .is_empty()
        );

        // A throwaway self-signed CA certificate (no key kept).
        vars.insert(
            "OIDC_CA_CERT_FILE",
            concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/test-ca.pem"),
        );
        assert_eq!(
            Config::from_vars(&vars)
                .unwrap()
                .oidc
                .unwrap()
                .ca_certs
                .len(),
            1
        );

        for path in [
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
            "/nonexistent/ca.pem",
        ] {
            vars.insert("OIDC_CA_CERT_FILE", path);
            assert!(matches!(
                Config::from_vars(&vars),
                Err(ConfigError::Invalid {
                    name: "OIDC_CA_CERT_FILE",
                    ..
                })
            ));
        }
    }

    #[test]
    fn ignores_empty_password_reset_url() {
        let mut vars = base();
        vars.insert("AUTH_PASSWORD_RESET_URL", "");
        assert!(
            Config::from_vars(&vars)
                .unwrap()
                .password_reset_url
                .is_none()
        );
    }
}

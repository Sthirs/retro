//! Test harness: the real router over a throw-away PostgreSQL database.
//!
//! A database comes from `TEST_DATABASE_URL` (a fresh database is created in
//! that server for every test) or, when unset, from a Postgres container
//! started with testcontainers (Docker or Podman).
#![allow(dead_code)]

use axum::{
    Router,
    body::Body,
    http::{Request, Response, header},
};
use http_body_util::BodyExt;
use retro_backend::{app, config::Config, connect, migrate, seed, state::AppState};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::Value;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner},
};
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
    pub db: DatabaseConnection,
    pub config: Config,
    _container: Option<ContainerAsync<Postgres>>,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    pub async fn with_config(customize: impl FnOnce(&mut Config)) -> Self {
        let (url, container) = database().await;
        let db = connect(&url).await.expect("connect");
        migrate(&db).await.expect("migrate");
        let mut config = Config::for_tests(&url);
        customize(&mut config);
        if config.seed_dev_users {
            seed::seed_dev_users(&db).await.expect("seed");
        }
        let router = app::router(AppState::new(db.clone(), config.clone()));
        Self {
            router,
            db,
            config,
            _container: container,
        }
    }

    pub async fn send(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("response");
        TestResponse::read(response).await
    }

    pub async fn get(&self, path: &str) -> TestResponse {
        self.send(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    pub async fn get_with_token(&self, path: &str, token: &str) -> TestResponse {
        self.send(
            Request::get(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    pub async fn post_json(&self, path: &str, body: Value) -> TestResponse {
        self.send(
            Request::post(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    pub async fn post_with_cookie(&self, path: &str, cookie: &str) -> TestResponse {
        self.send(
            Request::post(path)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Sends `method path` with a bearer token and an optional JSON body.
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> TestResponse {
        let builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        let request = match body {
            Some(json) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => builder.body(Body::empty()),
        };
        self.send(request.unwrap()).await
    }

    /// An access token for a seeded user (`alice`, `bob`).
    pub async fn token_for(&self, login: &str) -> String {
        let res = self.login(login, "password", false).await;
        assert_eq!(res.status, 200, "login {login}: {}", res.text());
        res.json()["accessToken"].as_str().unwrap().to_owned()
    }

    /// Creates a local user with password `password` and returns their token.
    pub async fn create_user(&self, username: &str, email: &str, name: &str) -> String {
        use retro_backend::{auth::password, entities::user};
        use sea_orm::{ActiveModelTrait, Set};
        let now = time::OffsetDateTime::now_utc();
        user::ActiveModel {
            id: Set(uuid::Uuid::now_v7()),
            email: Set(email.to_owned()),
            username: Set(Some(username.to_owned())),
            display_name: Set(name.to_owned()),
            avatar_url: Set(None),
            password_hash: Set(Some(password::hash("password").unwrap())),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await
        .expect("insert user");
        self.token_for(username).await
    }

    pub async fn login(&self, login: &str, password: &str, remember: bool) -> TestResponse {
        self.post_json(
            "/api/v1/auth/login",
            serde_json::json!({ "login": login, "password": password, "remember": remember }),
        )
        .await
    }
}

pub struct TestResponse {
    pub status: u16,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    async fn read(response: Response<Body>) -> Self {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The `Set-Cookie` header for `name`, if any.
    pub fn set_cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|v| v.starts_with(&format!("{name}=")))
            .map(str::to_owned)
    }

    /// `name=value` of a cookie set by this response, ready for a `Cookie` header.
    pub fn cookie_pair(&self, name: &str) -> Option<String> {
        self.set_cookie(name)
            .map(|c| c.split(';').next().unwrap_or_default().to_owned())
    }
}

async fn database() -> (String, Option<ContainerAsync<Postgres>>) {
    if let Ok(server) = std::env::var("TEST_DATABASE_URL") {
        let admin = connect(&server).await.expect("connect TEST_DATABASE_URL");
        let name = format!("retro_test_{}", uuid::Uuid::now_v7().simple());
        admin
            .execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create test database");
        let mut url = url::Url::parse(&server).expect("TEST_DATABASE_URL is a URL");
        url.set_path(&name);
        return (url.to_string(), None);
    }
    let container = Postgres::default()
        .with_tag("18-alpine")
        .start()
        .await
        .expect("start postgres container (is Docker or Podman running?)");
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    (
        format!("postgres://postgres:postgres@{host}:{port}/postgres"),
        Some(container),
    )
}

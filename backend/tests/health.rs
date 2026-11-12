mod common;

use common::TestApp;

#[tokio::test]
async fn liveness_and_readiness() {
    let app = TestApp::new().await;
    let live = app.get("/healthz").await;
    assert_eq!(live.status, 200);
    let ready = app.get("/readyz").await;
    assert_eq!(ready.status, 200, "{}", ready.text());
}

#[tokio::test]
async fn serves_openapi_document() {
    let app = TestApp::new().await;
    let doc = app.get("/api/v1/openapi.json").await;
    assert_eq!(doc.status, 200);
    let json = doc.json();
    assert!(json["paths"]["/api/v1/auth/login"].is_object());
    assert!(json["paths"]["/api/v1/me"].is_object());
}

#[tokio::test]
async fn responses_carry_a_request_id() {
    let app = TestApp::new().await;
    let res = app.get("/healthz").await;
    assert!(res.headers.contains_key("x-request-id"));
}

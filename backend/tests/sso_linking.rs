mod common;

use common::TestApp;
use retro_backend::{
    auth::oidc::Identity,
    domain::users::{SsoOutcome, sign_in_with_identity},
};

fn identity(subject: &str, email: &str) -> Identity {
    Identity {
        issuer: "http://idp.test".into(),
        subject: subject.into(),
        email: email.into(),
        name: Some("Alice From SSO".into()),
        picture: None,
    }
}

#[tokio::test]
async fn links_existing_account_by_email_then_reuses_link() {
    let app = TestApp::new().await;
    let (user, outcome) =
        sign_in_with_identity(&app.db, &identity("sub-a", "alice@retro.local"), false)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(outcome, SsoOutcome::Linked);
    assert_eq!(user.username.as_deref(), Some("alice"));
    assert_eq!(user.display_name, "Alice From SSO");

    let (again, outcome) =
        sign_in_with_identity(&app.db, &identity("sub-a", "alice@retro.local"), false)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(outcome, SsoOutcome::Existing);
    assert_eq!(again.id, user.id);
}

#[tokio::test]
async fn registers_unknown_users_only_when_allowed() {
    let app = TestApp::new().await;
    let carol = identity("sub-c", "carol@retro.local");
    assert!(
        sign_in_with_identity(&app.db, &carol, false)
            .await
            .unwrap()
            .is_none()
    );

    let (user, outcome) = sign_in_with_identity(&app.db, &carol, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome, SsoOutcome::Registered);
    assert_eq!(user.email, "carol@retro.local");
    assert!(user.password_hash.is_none());
    assert!(user.username.is_none());
}

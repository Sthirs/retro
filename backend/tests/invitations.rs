//! Invitations by email address (spec §6.1).

mod common;

use axum::{
    body::Body,
    http::{Request, header},
};
use common::TestApp;
use retro_backend::entities::invitation;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Ctx {
    app: TestApp,
    alice: String,
    bob: String,
    team: String,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let res = app
        .call(
            "POST",
            "/api/v1/teams",
            &alice,
            Some(json!({ "name": "Platform Team" })),
        )
        .await;
    let team = res.json()["id"].as_str().unwrap().to_owned();
    Ctx {
        app,
        alice,
        bob,
        team,
    }
}

impl Ctx {
    async fn invite(&self, token: &str, email: &str) -> Value {
        let res = self
            .app
            .call(
                "POST",
                &format!("/api/v1/teams/{}/invitations", self.team),
                token,
                Some(json!({ "email": email })),
            )
            .await;
        assert_eq!(res.status, 201, "{}", res.text());
        res.json()
    }

    /// Bob joins the team through its join link, as a member.
    async fn bob_joins(&self) {
        let link = self
            .app
            .get_with_token(
                &format!("/api/v1/teams/{}/join-link", self.team),
                &self.alice,
            )
            .await
            .json();
        let join_token = link["url"].as_str().unwrap().rsplit('/').next().unwrap();
        let res = self
            .app
            .call(
                "POST",
                &format!("/api/v1/join/{join_token}"),
                &self.bob,
                None,
            )
            .await;
        assert_eq!(res.status, 200, "{}", res.text());
    }

    async fn pending(&self) -> Vec<Value> {
        self.app
            .get_with_token(
                &format!("/api/v1/teams/{}/invitations", self.team),
                &self.alice,
            )
            .await
            .json()
            .as_array()
            .unwrap()
            .clone()
    }
}

fn token_of(invitation: &Value) -> String {
    invitation["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn invite_preview_and_accept() {
    let ctx = setup().await;
    let inv = ctx.invite(&ctx.alice, "  BOB@retro.local ").await;
    assert_eq!(inv["email"], "bob@retro.local");
    assert_eq!(inv["invitedBy"]["displayName"], "Alice Rossi");
    assert!(
        inv["url"]
            .as_str()
            .unwrap()
            .starts_with("http://retro.test/invite/")
    );

    // Listed for the team.
    let list = ctx
        .app
        .get_with_token(
            &format!("/api/v1/teams/{}/invitations", ctx.team),
            &ctx.alice,
        )
        .await
        .json();
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Bob sees it in /me.
    let me = ctx.app.get_with_token("/api/v1/me", &ctx.bob).await.json();
    assert_eq!(me["invitations"][0]["team"]["name"], "Platform Team");
    assert_eq!(
        me["invitations"][0]["invitedBy"]["displayName"],
        "Alice Rossi"
    );
    let token = token_of(&inv);
    assert_eq!(me["invitations"][0]["token"], token);

    // Preview.
    let preview = ctx
        .app
        .get_with_token(&format!("/api/v1/invitations/{token}"), &ctx.bob)
        .await
        .json();
    assert_eq!(preview["status"], "pending");
    assert_eq!(preview["email"], "bob@retro.local");
    assert_eq!(preview["team"]["name"], "Platform Team");
    assert_eq!(preview["memberCount"], 1);
    assert_eq!(preview["members"][0]["displayName"], "Alice Rossi");
    assert_eq!(preview["alreadyMember"], false);

    // Accept.
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/accept"),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(res.json()["name"], "Platform Team");
    let me = ctx.app.get_with_token("/api/v1/me", &ctx.bob).await.json();
    assert_eq!(me["teams"][0]["role"], "member");
    assert_eq!(me["invitations"], json!([]));

    // Accepting again while a member is harmless.
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/accept"),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200);

    // No longer pending for the team.
    let list = ctx
        .app
        .get_with_token(
            &format!("/api/v1/teams/{}/invitations", ctx.team),
            &ctx.alice,
        )
        .await
        .json();
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn any_holder_of_the_link_can_accept() {
    let ctx = setup().await;
    let inv = ctx.invite(&ctx.alice, "someone.else@acme.dev").await;
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{}/accept", token_of(&inv)),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn declining() {
    let ctx = setup().await;
    let inv = ctx.invite(&ctx.alice, "bob@retro.local").await;
    let token = token_of(&inv);
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/decline"),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 204);
    let me = ctx.app.get_with_token("/api/v1/me", &ctx.bob).await.json();
    assert_eq!(me["invitations"], json!([]));
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/accept"),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 410);
    assert_eq!(res.json()["code"], "invitation_used");
}

#[tokio::test]
async fn reinviting_returns_the_open_invitation() {
    let ctx = setup().await;
    let first = ctx.invite(&ctx.alice, "nina@acme.dev").await;
    let second = ctx.invite(&ctx.alice, "Nina@acme.dev").await;
    assert_eq!(first["id"], second["id"]);
}

#[tokio::test]
async fn rejects_members_and_bad_emails() {
    let ctx = setup().await;
    let path = format!("/api/v1/teams/{}/invitations", ctx.team);
    let res = ctx
        .app
        .call(
            "POST",
            &path,
            &ctx.alice,
            Some(json!({ "email": "alice@retro.local" })),
        )
        .await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "already_member");
    let res = ctx
        .app
        .call(
            "POST",
            &path,
            &ctx.alice,
            Some(json!({ "email": "not-an-email" })),
        )
        .await;
    assert_eq!(res.status, 400);
    assert_eq!(res.json()["code"], "invalid_email");
}

#[tokio::test]
async fn revoking() {
    let ctx = setup().await;
    let carol = ctx
        .app
        .create_user("carol", "carol@retro.local", "Carol Verdi")
        .await;
    // Bob joins as a member and invites someone.
    let link = ctx
        .app
        .call(
            "GET",
            &format!("/api/v1/teams/{}/join-link", ctx.team),
            &ctx.alice,
            None,
        )
        .await
        .json();
    let join_token = link["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    ctx.app
        .call(
            "POST",
            &format!("/api/v1/join/{join_token}"),
            &ctx.bob,
            None,
        )
        .await;
    ctx.app
        .call("POST", &format!("/api/v1/join/{join_token}"), &carol, None)
        .await;
    let by_bob = ctx.invite(&ctx.bob, "x@acme.dev").await;
    let by_alice = ctx.invite(&ctx.alice, "y@acme.dev").await;
    let revoke = |inv: &Value| {
        format!(
            "/api/v1/teams/{}/invitations/{}",
            ctx.team,
            inv["id"].as_str().unwrap()
        )
    };

    // Another member cannot revoke it; the inviter and admins can.
    assert_eq!(
        ctx.app
            .call("DELETE", &revoke(&by_bob), &carol, None)
            .await
            .status,
        403
    );
    assert_eq!(
        ctx.app
            .call("DELETE", &revoke(&by_alice), &ctx.bob, None)
            .await
            .status,
        403
    );
    assert_eq!(
        ctx.app
            .call("DELETE", &revoke(&by_bob), &ctx.bob, None)
            .await
            .status,
        204
    );
    assert_eq!(
        ctx.app
            .call("DELETE", &revoke(&by_alice), &ctx.alice, None)
            .await
            .status,
        204
    );

    let token = token_of(&by_bob);
    let preview = ctx
        .app
        .get_with_token(&format!("/api/v1/invitations/{token}"), &carol)
        .await
        .json();
    assert_eq!(preview["status"], "revoked");
    assert_eq!(preview["members"], json!([]));
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/accept"),
            &ctx.alice,
            None,
        )
        .await;
    // Alice is already a member: accepting is a no-op success.
    assert_eq!(res.status, 200);
    let outsider = ctx
        .app
        .create_user("dave", "dave@retro.local", "Dave")
        .await;
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/accept"),
            &outsider,
            None,
        )
        .await;
    assert_eq!(res.status, 410);
    assert_eq!(res.json()["code"], "invitation_revoked");
}

#[tokio::test]
async fn expired_invitations_fail() {
    let ctx = setup().await;
    let inv = ctx.invite(&ctx.alice, "bob@retro.local").await;
    ctx.app
        .db
        .execute_unprepared("UPDATE invitations SET expires_at = now() - interval '1 minute'")
        .await
        .unwrap();
    let token = token_of(&inv);

    let me = ctx.app.get_with_token("/api/v1/me", &ctx.bob).await.json();
    assert_eq!(me["invitations"], json!([]));
    let preview = ctx
        .app
        .get_with_token(&format!("/api/v1/invitations/{token}"), &ctx.bob)
        .await
        .json();
    assert_eq!(preview["status"], "expired");
    assert_eq!(preview["invitedBy"]["displayName"], "Alice Rossi");

    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{token}/accept"),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 410);
    assert_eq!(res.json()["code"], "invitation_expired");
    let stored = invitation::Entity::find()
        .one(&ctx.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, invitation::InvitationStatus::Expired);

    // A new invitation can be created for the same email.
    let again = ctx.invite(&ctx.alice, "bob@retro.local").await;
    assert_ne!(again["id"], inv["id"]);
}

#[tokio::test]
async fn unknown_tokens() {
    let ctx = setup().await;
    let res = ctx
        .app
        .get_with_token("/api/v1/invitations/unknown", &ctx.bob)
        .await;
    assert_eq!(res.status, 404);
    assert_eq!(res.json()["code"], "invitation_not_found");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_invites_make_one_invitation() {
    let ctx = setup().await;
    let path = format!("/api/v1/teams/{}/invitations", ctx.team);
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let router = ctx.app.router.clone();
        let request = Request::builder()
            .method("POST")
            .uri(&path)
            .header(header::AUTHORIZATION, format!("Bearer {}", ctx.alice))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "email": "nina@acme.dev" }).to_string()))
            .unwrap();
        tasks.push(tokio::spawn(router.oneshot(request)));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap().status(), 201);
    }
    let open = invitation::Entity::find()
        .filter(invitation::Column::Email.eq("nina@acme.dev"))
        .count(&ctx.app.db)
        .await
        .unwrap();
    assert_eq!(open, 1);
}

#[tokio::test]
async fn links_are_for_the_inviter_and_admins() {
    let ctx = setup().await;
    ctx.bob_joins().await;
    let by_alice = ctx.invite(&ctx.alice, "nina@acme.dev").await;
    let by_bob = ctx.invite(&ctx.bob, "omar@acme.dev").await;
    assert!(by_bob["url"].is_string());

    let list = ctx
        .app
        .get_with_token(&format!("/api/v1/teams/{}/invitations", ctx.team), &ctx.bob)
        .await
        .json();
    let url_of = |id: &Value| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == *id)
            .unwrap()["url"]
            .clone()
    };
    assert_eq!(url_of(&by_alice["id"]), Value::Null);
    assert_eq!(url_of(&by_bob["id"]), by_bob["url"]);
    // Admins see every link.
    for inv in ctx.pending().await {
        assert!(inv["url"].is_string());
    }

    // A member cannot get another person's open invitation back by re-inviting.
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/teams/{}/invitations", ctx.team),
            &ctx.bob,
            Some(json!({ "email": "nina@acme.dev" })),
        )
        .await;
    assert_eq!(res.status, 409);
    assert_eq!(res.json()["code"], "already_invited");
    // The admin gets the member's invitation back, with its link.
    let again = ctx.invite(&ctx.alice, "omar@acme.dev").await;
    assert_eq!(again["id"], by_bob["id"]);
    assert_eq!(again["url"], by_bob["url"]);
    assert_eq!(again["invitedBy"]["displayName"], "Bob Bianchi");
}

#[tokio::test]
async fn reinviting_does_not_use_the_quota() {
    let ctx = setup().await;
    // More than the hourly quota (100) of re-invites for one email.
    for _ in 0..105 {
        ctx.invite(&ctx.alice, "nina@acme.dev").await;
    }
    ctx.invite(&ctx.alice, "omar@acme.dev").await;
}

#[tokio::test]
async fn a_member_accepting_keeps_someone_elses_invitation_open() {
    let ctx = setup().await;
    ctx.bob_joins().await;
    let inv = ctx.invite(&ctx.alice, "nina@acme.dev").await;
    let res = ctx
        .app
        .call(
            "POST",
            &format!("/api/v1/invitations/{}/accept", token_of(&inv)),
            &ctx.bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200);
    let pending = ctx.pending().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["id"], inv["id"]);
}

#[tokio::test]
async fn joining_by_link_closes_the_invitation() {
    let ctx = setup().await;
    ctx.invite(&ctx.alice, "bob@retro.local").await;
    ctx.bob_joins().await;
    assert_eq!(ctx.pending().await, Vec::<Value>::new());
    let me = ctx.app.get_with_token("/api/v1/me", &ctx.bob).await.json();
    assert_eq!(me["invitations"], json!([]));
}

#[tokio::test]
async fn mixed_case_invitations_reach_the_invitee() {
    let ctx = setup().await;
    ctx.invite(&ctx.alice, "Bob@Retro.Local").await;
    let me = ctx.app.get_with_token("/api/v1/me", &ctx.bob).await.json();
    assert_eq!(me["invitations"][0]["team"]["name"], "Platform Team");
}

#[tokio::test]
async fn token_responses_are_not_cached() {
    let ctx = setup().await;
    let inv = ctx.invite(&ctx.alice, "nina@acme.dev").await;
    let paths = [
        "/api/v1/me".to_owned(),
        format!("/api/v1/teams/{}/invitations", ctx.team),
        format!("/api/v1/teams/{}/join-link", ctx.team),
        format!("/api/v1/invitations/{}", token_of(&inv)),
    ];
    for path in paths {
        let res = ctx.app.get_with_token(&path, &ctx.alice).await;
        assert_eq!(res.status, 200, "{path}");
        assert_eq!(res.headers["cache-control"], "no-store", "{path}");
    }
}

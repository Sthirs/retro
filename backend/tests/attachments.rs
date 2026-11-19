//! Image attachments of reflections and tasks (spec §13, §14.4, ADR-0013).

mod common;

use axum::{
    body::Body,
    http::{Request, header},
};
use common::{TestApp, TestResponse};
use image::{ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};
use retro_backend::{
    domain::{attachments, tasks},
    entities::{attachment, task::TaskStatus},
    error::AppError,
    services::images::MAX_UPLOAD_BYTES,
};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait, Set, Statement,
    TransactionTrait,
};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba([200, 30, 90, 255]));
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(&image, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    out
}

fn img(id: &str) -> String {
    format!(r#"<p>look <img data-attachment-id="{id}" alt="chart"></p>"#)
}

async fn create_team(app: &TestApp, token: &str, name: &str) -> Uuid {
    let res = app
        .call(
            "POST",
            "/api/v1/teams",
            token,
            Some(json!({ "name": name })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()["id"].as_str().unwrap().parse().unwrap()
}

async fn user_id(app: &TestApp, token: &str) -> Uuid {
    app.get_with_token("/api/v1/me", token).await.json()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn join(app: &TestApp, admin: &str, team: Uuid, token: &str) {
    let link = app
        .call(
            "GET",
            &format!("/api/v1/teams/{team}/join-link"),
            admin,
            None,
        )
        .await
        .json();
    let join_token = link["url"].as_str().unwrap().rsplit('/').next().unwrap();
    let res = app
        .call("POST", &format!("/api/v1/join/{join_token}"), token, None)
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
}

async fn upload(app: &TestApp, token: &str, team: Uuid, bytes: Vec<u8>) -> TestResponse {
    app.send(
        Request::post(format!("/api/v1/teams/{team}/attachments"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "image/png")
            .body(Body::from(bytes))
            .unwrap(),
    )
    .await
}

/// Uploads a small PNG and returns the attachment id.
async fn uploaded(app: &TestApp, token: &str, team: Uuid) -> String {
    let res = upload(app, token, team, png(3, 2)).await;
    assert_eq!(res.status, 201, "{}", res.text());
    res.json()["id"].as_str().unwrap().to_owned()
}

async fn fetch(app: &TestApp, token: &str, id: &str) -> TestResponse {
    app.get_with_token(&format!("/api/v1/attachments/{id}"), token)
        .await
}

async fn edit_task(app: &TestApp, token: &str, task: Uuid, content: &str) -> TestResponse {
    app.call(
        "PATCH",
        &format!("/api/v1/tasks/{task}"),
        token,
        Some(json!({ "content": content })),
    )
    .await
}

fn code(res: &TestResponse) -> Value {
    res.json()["code"].clone()
}

/// Inserts an attachment row directly, `age` old.
async fn insert_attachment(app: &TestApp, team: Uuid, uploader: Uuid, age: time::Duration) -> Uuid {
    let id = Uuid::now_v7();
    attachment::ActiveModel {
        id: Set(id),
        team_id: Set(team),
        uploader_id: Set(uploader),
        content_type: Set("image/webp".into()),
        animated: Set(false),
        width: Set(1),
        height: Set(1),
        size_bytes: Set(4),
        sha256: Set("00".into()),
        data: Set(b"RIFF".to_vec()),
        created_at: Set(OffsetDateTime::now_utc() - age),
    }
    .insert(&app.db)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn uploads_are_converted_to_webp_and_served_safely() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;

    let res = upload(&app, &alice, team, png(3, 2)).await;
    assert_eq!(res.status, 201, "{}", res.text());
    let body = res.json();
    assert_eq!(body["contentType"], "image/webp");
    assert_eq!(
        (body["width"].as_i64(), body["height"].as_i64()),
        (Some(3), Some(2))
    );
    assert_eq!(body["animated"], false);
    assert!(body.get("uploaderId").is_none());
    let id = body["id"].as_str().unwrap();

    let res = fetch(&app, &alice, id).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(&res.body[..4], b"RIFF");
    assert_eq!(&res.body[8..12], b"WEBP");
    assert_eq!(res.headers["content-type"], "image/webp");
    assert_eq!(res.headers["x-content-type-options"], "nosniff");
    assert_eq!(
        res.headers["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    assert_eq!(res.headers["cross-origin-resource-policy"], "same-origin");
    assert!(
        res.headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    let cache = res.headers["cache-control"].to_str().unwrap();
    assert!(
        cache.contains("private") && cache.contains("immutable"),
        "{cache}"
    );

    // Revalidation by ETag answers 304 without the bytes, still cacheable.
    let etag = res.headers["etag"].to_str().unwrap().to_owned();
    let path = format!("/api/v1/attachments/{id}");
    let res = app.get_if_none_match(&path, &alice, &etag).await;
    assert_eq!(res.status, 304);
    assert!(res.body.is_empty());
    assert_eq!(res.headers["etag"], etag.as_str());
    assert_eq!(res.headers["cache-control"], cache);
    // Only for someone who may see the image.
    let res = app
        .get_if_none_match(
            &format!("/api/v1/attachments/{}", Uuid::now_v7()),
            &alice,
            &etag,
        )
        .await;
    assert_eq!(res.status, 404);

    // Errors stay uncached.
    let res = fetch(&app, &alice, &Uuid::now_v7().to_string()).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "attachment_not_found");
    assert_eq!(res.headers["cache-control"], "no-store");
    let res = fetch(&app, "", id).await;
    assert_eq!(res.status, 401);
}

#[tokio::test]
async fn non_members_can_neither_upload_nor_read() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    let id = uploaded(&app, &alice, team).await;

    let res = upload(&app, &bob, team, png(1, 1)).await;
    assert_eq!(res.status, 404);
    assert_eq!(code(&res), "team_not_found");
    let res = fetch(&app, &bob, &id).await;
    assert_eq!(res.status, 404);
}

#[tokio::test]
async fn rejects_unsupported_broken_and_oversize_images() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;

    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#.to_vec();
    let res = upload(&app, &alice, team, svg).await;
    assert_eq!(res.status, 415);
    assert_eq!(code(&res), "unsupported_image_type");

    // Magic bytes followed by script: the "header" is nonsense either way.
    let res = upload(
        &app,
        &alice,
        team,
        b"GIF89a<script>alert(1)</script>".to_vec(),
    )
    .await;
    assert_eq!(res.status, 400, "{}", res.text());

    let mut truncated = png(4, 4);
    truncated.truncate(truncated.len() / 2);
    let res = upload(&app, &alice, team, truncated).await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "invalid_image");

    let mut huge = png(1, 1);
    huge[16..24].copy_from_slice(&[0, 0, 0x30, 0, 0, 0, 0x30, 0]);
    let res = upload(&app, &alice, team, huge).await;
    assert_eq!(res.status, 400);
    assert!(
        code(&res) == "image_dimensions_too_large" || code(&res) == "invalid_image",
        "{}",
        res.text()
    );

    let res = upload(&app, &alice, team, vec![0u8; MAX_UPLOAD_BYTES + 1]).await;
    assert_eq!(res.status, 413);
    assert_eq!(code(&res), "image_too_large");
    assert_eq!(res.headers["content-type"], "application/problem+json");

    assert_eq!(attachment::Entity::find().count(&app.db).await.unwrap(), 0);
}

#[tokio::test]
async fn uploads_are_rate_limited_and_pending_ones_capped() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;

    for _ in 0..attachments::MAX_PENDING_PER_USER {
        insert_attachment(&app, team, alice_id, time::Duration::ZERO).await;
    }
    let res = upload(&app, &alice, team, png(1, 1)).await;
    assert_eq!(res.status, 429);
    assert_eq!(code(&res), "too_many_pending_attachments");

    for _ in 0..60 {
        app.state
            .upload_limiter
            .record_failure(&alice_id.to_string());
    }
    let res = upload(&app, &alice, team, png(1, 1)).await;
    assert_eq!(res.status, 429);
    assert_eq!(code(&res), "too_many_requests");
}

#[tokio::test]
async fn task_images_are_visible_to_the_team_once_referenced() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;
    let task = tasks::insert(&app.db, team, alice_id, TaskStatus::Active, "<p>t</p>")
        .await
        .unwrap();
    let id = uploaded(&app, &alice, team).await;

    // Pending: only the uploader sees it.
    assert_eq!(fetch(&app, &bob, &id).await.status, 404);

    let res = edit_task(&app, &alice, task.id, &img(&id)).await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(
        res.json()["contentHtml"],
        format!(r#"<p>look <img data-attachment-id="{id}" alt="chart" width="3" height="2"></p>"#)
    );
    assert_eq!(fetch(&app, &bob, &id).await.status, 200);

    // Bob may keep alice's image when he edits the task…
    let res = edit_task(&app, &bob, task.id, &format!("{}<p>more</p>", img(&id))).await;
    assert_eq!(res.status, 200, "{}", res.text());

    // …and removing it unreferences it again.
    let res = edit_task(&app, &bob, task.id, "<p>no image</p>").await;
    assert_eq!(res.status, 200, "{}", res.text());
    assert_eq!(fetch(&app, &bob, &id).await.status, 404);
    assert_eq!(fetch(&app, &alice, &id).await.status, 200);
}

#[tokio::test]
async fn items_cannot_reference_foreign_or_other_peoples_pending_images() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;
    let other = create_team(&app, &alice, "Other").await;
    join(&app, &alice, team, &bob).await;
    let task = tasks::insert(&app.db, team, alice_id, TaskStatus::Active, "<p>t</p>")
        .await
        .unwrap();

    // Alice's pending image: bob cannot copy its id into a task to see it.
    let pending = uploaded(&app, &alice, team).await;
    let res = edit_task(&app, &bob, task.id, &img(&pending)).await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "attachment_not_found");
    assert_eq!(fetch(&app, &bob, &pending).await.status, 404);

    // Another team's image, even alice's own.
    let foreign = uploaded(&app, &alice, other).await;
    let res = edit_task(&app, &alice, task.id, &img(&foreign)).await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "attachment_not_found");

    // An id that does not exist.
    let res = edit_task(&app, &alice, task.id, &img(&Uuid::now_v7().to_string())).await;
    assert_eq!(res.status, 400);
}

#[tokio::test]
async fn reflection_images_stay_private_until_the_reflections_are_revealed() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings/{meeting}/join"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let next = format!("/api/v1/meetings/{meeting}/phase/next");
    assert_eq!(app.call("POST", &next, &alice, None).await.status, 200);

    let id = uploaded(&app, &bob, team).await;
    // Alice cannot reference bob's image in her own reflection.
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{meeting}/reflections"),
            &alice,
            Some(json!({ "column": "plus", "content": img(&id) })),
        )
        .await;
    assert_eq!(res.status, 400);
    assert_eq!(code(&res), "attachment_not_found");

    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{meeting}/reflections"),
            &bob,
            Some(json!({ "column": "plus", "content": img(&id) })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let mine = &res.json()["reflections"][0]["content"];
    assert!(
        mine.as_str().unwrap().contains(r#"width="3" height="2""#),
        "{mine}"
    );

    // Reflect: the image is as private as the reflection.
    assert_eq!(fetch(&app, &alice, &id).await.status, 404);
    assert_eq!(fetch(&app, &bob, &id).await.status, 200);

    // Group reveals the reflections, and their images.
    assert_eq!(app.call("POST", &next, &alice, None).await.status, 200);
    assert_eq!(fetch(&app, &alice, &id).await.status, 200);
}

#[tokio::test]
async fn cleanup_removes_only_old_unreferenced_images() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;
    let day = time::Duration::hours(25);
    let old_orphan = insert_attachment(&app, team, alice_id, day).await;
    let new_orphan = insert_attachment(&app, team, alice_id, time::Duration::ZERO).await;
    let old_used = insert_attachment(&app, team, alice_id, day).await;
    tasks::insert(
        &app.db,
        team,
        alice_id,
        TaskStatus::Active,
        &img(&old_used.to_string()),
    )
    .await
    .unwrap();

    attachments::cleanup(&app.db).await.unwrap();
    let left = |id| attachment::Entity::find_by_id(id).one(&app.db);
    assert!(left(old_orphan).await.unwrap().is_none());
    assert!(left(new_orphan).await.unwrap().is_some());
    assert!(left(old_used).await.unwrap().is_some());
}

#[tokio::test]
async fn deleting_the_team_deletes_its_images() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let team = create_team(&app, &alice, "Platform").await;
    uploaded(&app, &alice, team).await;
    let res = app
        .call("DELETE", &format!("/api/v1/teams/{team}"), &alice, None)
        .await;
    assert!(res.status == 204 || res.status == 200, "{}", res.text());
    assert_eq!(attachment::Entity::find().count(&app.db).await.unwrap(), 0);
}

#[tokio::test]
async fn cleanup_skips_images_a_save_is_referencing() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;
    let old = insert_attachment(&app, team, alice_id, time::Duration::hours(25)).await;

    // A save holds the image, not yet committed, while the cleanup runs.
    let txn = app.db.begin().await.unwrap();
    let task = tasks::insert(
        &txn,
        team,
        alice_id,
        TaskStatus::Active,
        &img(&old.to_string()),
    )
    .await
    .unwrap();
    let deleted = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        attachments::cleanup(&app.db),
    )
    .await
    .expect("cleanup waits on the save")
    .unwrap();
    assert_eq!(deleted, 0);
    txn.commit().await.unwrap();

    assert!(
        attachment::Entity::find_by_id(old)
            .one(&app.db)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(attachments::cleanup(&app.db).await.unwrap(), 0);
    // The reference row the save inserted survives too.
    assert!(task.content.contains(&old.to_string()), "{}", task.content);
    let refs = app
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM task_attachments WHERE task_id = $1 AND attachment_id = $2",
            [task.id.into(), old.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refs.try_get::<i64>("", "n").unwrap(), 1);
    assert_eq!(fetch(&app, &alice, &old.to_string()).await.status, 200);
}

#[tokio::test]
async fn parallel_uploads_cannot_pass_the_pending_cap() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let alice_id = user_id(&app, &alice).await;
    let team = create_team(&app, &alice, "Platform").await;
    for _ in 1..attachments::MAX_PENDING_PER_USER {
        insert_attachment(&app, team, alice_id, time::Duration::ZERO).await;
    }

    let results = futures_util::future::join_all(
        (0..5).map(|_| attachments::upload(&app.db, team, alice_id, png(1, 1))),
    )
    .await;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    for err in results.into_iter().filter_map(Result::err) {
        assert!(
            matches!(err, AppError::Limited("too_many_pending_attachments")),
            "{err:?}"
        );
    }
}

#[tokio::test]
async fn reflection_images_stay_private_when_the_meeting_ends_during_reflect() {
    let app = TestApp::new().await;
    let alice = app.token_for("alice").await;
    let bob = app.token_for("bob").await;
    let team = create_team(&app, &alice, "Platform").await;
    join(&app, &alice, team, &bob).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings"),
            &alice,
            None,
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());
    let meeting = res.json()["id"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "POST",
            &format!("/api/v1/teams/{team}/meetings/{meeting}/join"),
            &bob,
            None,
        )
        .await;
    assert_eq!(res.status, 200, "{}", res.text());
    let next = format!("/api/v1/meetings/{meeting}/phase/next");
    assert_eq!(app.call("POST", &next, &alice, None).await.status, 200);

    let id = uploaded(&app, &bob, team).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{meeting}/reflections"),
            &bob,
            Some(json!({ "column": "plus", "content": img(&id) })),
        )
        .await;
    assert_eq!(res.status, 201, "{}", res.text());

    let res = app
        .call(
            "POST",
            &format!("/api/v1/meetings/{meeting}/end"),
            &alice,
            None,
        )
        .await;
    assert!(res.status < 300, "{}", res.text());
    assert_eq!(fetch(&app, &alice, &id).await.status, 404);
    assert_eq!(fetch(&app, &bob, &id).await.status, 200);
}

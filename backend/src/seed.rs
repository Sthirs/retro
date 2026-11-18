//! Development seed data. Only runs when `SEED_DEV_USERS=true`, which the
//! configuration refuses in production.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, TransactionTrait,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::password,
    domain::{tasks, teams},
    entities::{task::TaskStatus, team, user},
    error::AppError,
};

pub struct DevUser {
    pub username: &'static str,
    pub email: &'static str,
    pub display_name: &'static str,
}

/// Password of every seeded account. Matches the dev identity provider's users.
// codeql[rust/hard-coded-cryptographic-value] Dev-only; seeding is refused in production.
pub const DEV_PASSWORD: &str = "password";

pub const DEV_USERS: &[DevUser] = &[
    DevUser {
        username: "alice",
        email: "alice@retro.local",
        display_name: "Alice Rossi",
    },
    DevUser {
        username: "bob",
        email: "bob@retro.local",
        display_name: "Bob Bianchi",
    },
    // Not in the dev identity provider: password sign-in only. The end-to-end
    // account spec owns this user, so it can rename it and change its picture
    // without disturbing alice and bob.
    DevUser {
        username: "dana",
        email: "dana@retro.local",
        display_name: "Dana Neri",
    },
    // Also password only. The end-to-end tasks spec owns these two, so it can
    // build boards with several members without touching alice, bob and dana.
    DevUser {
        username: "erin",
        email: "erin@retro.local",
        display_name: "Erin Gallo",
    },
    DevUser {
        username: "finn",
        email: "finn@retro.local",
        display_name: "Finn Moretti",
    },
    // Also password only. The end-to-end teams spec owns these two: it needs
    // people without any team (onboarding), so it wipes their teams.
    DevUser {
        username: "gina",
        email: "gina@retro.local",
        display_name: "Gina Ferri",
    },
    DevUser {
        username: "hugo",
        email: "hugo@retro.local",
        display_name: "Hugo Conti",
    },
    // Also password only. The end-to-end meetings spec owns these three: ivan
    // and jade run meetings together, kai joins through a meeting invite link.
    DevUser {
        username: "ivan",
        email: "ivan@retro.local",
        display_name: "Ivan Galli",
    },
    DevUser {
        username: "jade",
        email: "jade@retro.local",
        display_name: "Jade Marino",
    },
    DevUser {
        username: "kai",
        email: "kai@retro.local",
        display_name: "Kai Russo",
    },
    // Also password only. The end-to-end reflect spec owns these three: lena
    // facilitates, milo writes, nora joins late and takes over as an admin.
    DevUser {
        username: "lena",
        email: "lena@retro.local",
        display_name: "Lena Fontana",
    },
    DevUser {
        username: "milo",
        email: "milo@retro.local",
        display_name: "Milo Testa",
    },
    DevUser {
        username: "nora",
        email: "nora@retro.local",
        display_name: "Nora Pellegrini",
    },
    // Also password only. The end-to-end group spec owns these two: omar
    // facilitates, pia groups cards with him.
    DevUser {
        username: "omar",
        email: "omar@retro.local",
        display_name: "Omar Ricci",
    },
    DevUser {
        username: "pia",
        email: "pia@retro.local",
        display_name: "Pia Longo",
    },
    DevUser {
        username: "quinn",
        email: "quinn@retro.local",
        display_name: "Quinn Serra",
    },
    DevUser {
        username: "rosa",
        email: "rosa@retro.local",
        display_name: "Rosa Villa",
    },
    DevUser {
        username: "sami",
        email: "sami@retro.local",
        display_name: "Sami Caruso",
    },
    DevUser {
        username: "tess",
        email: "tess@retro.local",
        display_name: "Tess Barbieri",
    },
];

/// Creates the dev users, or resets their name and password. Idempotent.
pub async fn seed_dev_users(db: &impl ConnectionTrait) -> Result<(), AppError> {
    let hash = password::hash(DEV_PASSWORD)?;
    let now = OffsetDateTime::now_utc();
    for dev in DEV_USERS {
        let existing = user::Entity::find()
            .filter(user::Column::Email.eq(dev.email))
            .one(db)
            .await?;
        match existing {
            Some(u) => {
                let mut active: user::ActiveModel = u.into();
                active.username = Set(Some(dev.username.into()));
                active.display_name = Set(dev.display_name.into());
                active.display_name_custom = Set(false);
                active.password_hash = Set(Some(hash.clone()));
                active.updated_at = Set(now);
                active.update(db).await?;
            }
            None => {
                user::ActiveModel {
                    id: Set(Uuid::now_v7()),
                    email: Set(dev.email.into()),
                    username: Set(Some(dev.username.into())),
                    display_name: Set(dev.display_name.into()),
                    avatar_url: Set(None),
                    display_name_custom: Set(false),
                    picture_version: Set(None),
                    password_hash: Set(Some(hash.clone())),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(db)
                .await?;
            }
        }
    }
    tracing::info!(count = DEV_USERS.len(), "seeded dev users");
    Ok(())
}

/// A demo team: created by `admin`, joined by `members`, with `tasks` in board
/// order. Users are dev usernames.
pub struct DemoTeam {
    pub name: &'static str,
    pub admin: &'static str,
    pub members: &'static [&'static str],
    pub tasks: &'static [DemoTask],
}

/// A demo task. In `text`, `@username` becomes a mention of that dev user.
pub struct DemoTask {
    pub status: TaskStatus,
    pub author: &'static str,
    pub text: &'static str,
}

const fn task(status: TaskStatus, author: &'static str, text: &'static str) -> DemoTask {
    DemoTask {
        status,
        author,
        text,
    }
}

/// Demo data for people trying the app by hand. The end-to-end suite never
/// touches alice and bob, so it survives test runs.
pub const DEMO_TEAMS: &[DemoTeam] = &[
    DemoTeam {
        name: "Platform",
        admin: "alice",
        members: &["bob"],
        tasks: &[
            task(TaskStatus::Future, "alice", "Plan the PostgreSQL upgrade"),
            task(
                TaskStatus::Future,
                "bob",
                "Try pairing on code reviews for a sprint",
            ),
            task(
                TaskStatus::Future,
                "bob",
                "Write down the on-call handover, @alice",
            ),
            task(
                TaskStatus::Stuck,
                "alice",
                "Staging access for the new contractors: waiting on IT, @bob",
            ),
            task(TaskStatus::Active, "alice", "Get CI under 10 minutes, @bob"),
            task(TaskStatus::Active, "bob", "Alert on queue depth"),
            task(TaskStatus::Done, "alice", "Move the standup to 9:45"),
            task(
                TaskStatus::Done,
                "bob",
                "Remove unused feature flags, @alice",
            ),
        ],
    },
    DemoTeam {
        name: "Mobile",
        admin: "bob",
        members: &["alice"],
        tasks: &[
            task(TaskStatus::Future, "bob", "Prototype offline mode"),
            task(
                TaskStatus::Stuck,
                "alice",
                "App Store rejected the build: waiting on legal",
            ),
            task(
                TaskStatus::Active,
                "bob",
                "Fix the crash when rotating the camera on Android, @alice",
            ),
            task(TaskStatus::Done, "alice", "Ship dark mode"),
        ],
    },
];

/// Creates the demo teams and their tasks. A team that its admin already
/// created (same name) is left as it is, so manual changes survive restarts;
/// `scripts/db-reset.sh` brings the demo back. Runs after [`seed_dev_users`].
pub async fn seed_dev_data(db: &(impl TransactionTrait + ConnectionTrait)) -> Result<(), AppError> {
    let mut created = 0;
    for demo in DEMO_TEAMS {
        let admin = dev_user(db, demo.admin).await?;
        let exists = team::Entity::find()
            .filter(team::Column::Name.eq(demo.name))
            .filter(team::Column::CreatedBy.eq(admin.id))
            .one(db)
            .await?
            .is_some();
        if exists {
            continue;
        }
        let summary = teams::create(db, admin.id, demo.name).await?;
        let mut people = vec![admin];
        for username in demo.members {
            let member = dev_user(db, username).await?;
            teams::add_member(db, summary.id, member.id).await?;
            people.push(member);
        }
        for t in demo.tasks {
            let author = people
                .iter()
                .find(|u| u.username.as_deref() == Some(t.author))
                .ok_or_else(|| {
                    AppError::Internal(anyhow::anyhow!("demo author {} not in team", t.author))
                })?;
            let content = format!("<p>{}</p>", with_mentions(t.text, &people));
            tasks::insert(db, summary.id, author.id, t.status, &content).await?;
        }
        created += 1;
    }
    tracing::info!(created, "seeded dev demo teams");
    Ok(())
}

async fn dev_user(db: &impl ConnectionTrait, username: &str) -> Result<user::Model, AppError> {
    let email = DEV_USERS
        .iter()
        .find(|u| u.username == username)
        .map(|u| u.email)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("unknown dev user {username}")))?;
    user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("dev user {username} is not seeded")))
}

/// Replaces `@username` with a mention span of that team member.
fn with_mentions(text: &str, people: &[user::Model]) -> String {
    people.iter().fold(text.to_owned(), |acc, u| {
        let Some(username) = u.username.as_deref() else {
            return acc;
        };
        acc.replace(
            &format!("@{username}"),
            &format!(
                r#"<span data-type="mention" data-id="{}" data-label="{name}">@{name}</span>"#,
                u.id,
                name = u.display_name,
            ),
        )
    })
}

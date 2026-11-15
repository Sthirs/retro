//! The tasks board: tasks, their authors and mentions (spec §9, §13).

use std::collections::HashMap;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, SqlErr, TransactionSession, TransactionTrait, sea_query::Query,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    domain::{ordering::key_between, rich_text, teams},
    entities::{
        task::{self, TaskStatus},
        task_mention,
        team::{self},
        team_member, user,
    },
    error::AppError,
    ws,
};

/// Narrows the board; every field set must match (AND).
#[derive(Debug, Clone, Default)]
pub struct BoardFilter {
    /// Only tasks that mention any of these users (none: no constraint).
    pub mentioned: Vec<Uuid>,
    /// Only tasks written by this user.
    pub author: Option<Uuid>,
    /// Only tasks attached to this meeting.
    pub meeting: Option<Uuid>,
}

/// A task with what the board shows next to it.
#[derive(Debug, Clone)]
pub struct TaskView {
    pub task: task::Model,
    pub author: user::Model,
    pub mentions: Vec<user::Model>,
}

/// The team's tasks, ordered by `(status, position, id)`. Non-members get
/// `team_not_found`. Authors and mentions are loaded in bulk.
pub async fn list(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    actor: Uuid,
    filter: BoardFilter,
) -> Result<Vec<TaskView>, AppError> {
    teams::require_member(db, team_id, actor).await?;

    let mut query = task::Entity::find().filter(task::Column::TeamId.eq(team_id));
    if let Some(author) = filter.author {
        query = query.filter(task::Column::AuthorId.eq(author));
    }
    if let Some(meeting) = filter.meeting {
        query = query.filter(task::Column::MeetingId.eq(meeting));
    }
    if !filter.mentioned.is_empty() {
        query = query.filter(
            task::Column::Id.in_subquery(
                Query::select()
                    .column(task_mention::Column::TaskId)
                    .from(task_mention::Entity)
                    .and_where(task_mention::Column::UserId.is_in(filter.mentioned))
                    .to_owned(),
            ),
        );
    }
    let rows = query
        .order_by_asc(task::Column::Status)
        .order_by_asc(task::Column::Position)
        .order_by_asc(task::Column::Id)
        .all(db)
        .await?;

    views(db, rows).await
}

/// Attaches author and mentions to `rows`, loading both in bulk.
async fn views(
    db: &impl ConnectionTrait,
    rows: Vec<task::Model>,
) -> Result<Vec<TaskView>, AppError> {
    let mut mention_ids: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    if !rows.is_empty() {
        let mentions = task_mention::Entity::find()
            .filter(task_mention::Column::TaskId.is_in(rows.iter().map(|t| t.id)))
            .order_by_asc(task_mention::Column::TaskId)
            .order_by_asc(task_mention::Column::UserId)
            .all(db)
            .await?;
        for m in mentions {
            mention_ids.entry(m.task_id).or_default().push(m.user_id);
        }
    }

    let people = teams::users_by_id(
        db,
        rows.iter()
            .map(|t| t.author_id)
            .chain(mention_ids.values().flatten().copied()),
    )
    .await?;

    rows.into_iter()
        .map(|task| {
            let author = people
                .get(&task.author_id)
                .cloned()
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("task author is missing")))?;
            let mentions = mention_ids
                .get(&task.id)
                .into_iter()
                .flatten()
                .filter_map(|id| people.get(id).cloned())
                .collect();
            Ok(TaskView {
                task,
                author,
                mentions,
            })
        })
        .collect()
}

/// A single task with its author and mentions, as `list` returns it.
pub async fn view(db: &impl ConnectionTrait, task: task::Model) -> Result<TaskView, AppError> {
    views(db, vec![task])
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("task is missing")))
}

/// Adds a task at the end of its column. The content is sanitized; mentions of
/// users who are not team members are dropped.
pub async fn insert(
    db: &(impl TransactionTrait + ConnectionTrait),
    team_id: Uuid,
    author_id: Uuid,
    status: TaskStatus,
    raw_content: &str,
) -> Result<task::Model, AppError> {
    insert_from(db, team_id, author_id, status, raw_content, None).await
}

/// [`insert`] for a request from the tab `client` (see [`publish`]).
pub async fn insert_from(
    db: &(impl TransactionTrait + ConnectionTrait),
    team_id: Uuid,
    author_id: Uuid,
    status: TaskStatus,
    raw_content: &str,
    client: Option<Uuid>,
) -> Result<task::Model, AppError> {
    let content = rich_text::sanitize(raw_content)?;
    let txn = db.begin().await?;

    // Serialises inserts into the same team, so two tasks never pick the same
    // end-of-column key.
    team::Entity::find_by_id(team_id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound("team_not_found"))?;

    let last = task::Entity::find()
        .filter(task::Column::TeamId.eq(team_id))
        .filter(task::Column::Status.eq(status))
        .order_by_desc(task::Column::Position)
        .one(&txn)
        .await?
        .map(|t| t.position);
    let position = key_between(last.as_deref(), None)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("cannot order after the last task")))?;

    let now = OffsetDateTime::now_utc();
    let task = task::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        meeting_id: Set(None),
        group_id: Set(None),
        author_id: Set(author_id),
        content: Set(content.html),
        status: Set(status),
        position: Set(position),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&txn)
    .await?;

    insert_mentions(&txn, team_id, task.id, content.mention_ids).await?;
    publish(&txn, &task, "task.created", client).await?;

    txn.commit().await?;
    Ok(task)
}

/// Tells the team's boards that a task changed (spec §10.1, §11.2). `client`
/// is the tab that made the change, which skips its own echo.
async fn publish(
    db: &impl ConnectionTrait,
    task: &task::Model,
    kind: &str,
    client: Option<Uuid>,
) -> Result<(), AppError> {
    ws::publish_team(
        db,
        task.team_id,
        kind,
        serde_json::json!({ "taskId": task.id, "status": task.status, "clientId": client }),
    )
    .await?;
    Ok(())
}

/// Records the mentions of `task_id`, keeping only team members.
async fn insert_mentions(
    db: &impl ConnectionTrait,
    team_id: Uuid,
    task_id: Uuid,
    mention_ids: Vec<Uuid>,
) -> Result<(), AppError> {
    if mention_ids.is_empty() {
        return Ok(());
    }
    let members: Vec<Uuid> = team_member::Entity::find()
        .filter(team_member::Column::TeamId.eq(team_id))
        .filter(team_member::Column::UserId.is_in(mention_ids.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.user_id)
        .collect();
    let rows: Vec<task_mention::ActiveModel> = mention_ids
        .into_iter()
        .filter(|id| members.contains(id))
        .map(|user_id| task_mention::ActiveModel {
            task_id: Set(task_id),
            user_id: Set(user_id),
        })
        .collect();
    if !rows.is_empty() {
        task_mention::Entity::insert_many(rows).exec(db).await?;
    }
    Ok(())
}

/// Loads a task for a write by `actor` and locks it: team row first, then the
/// task row (the same order as `insert`). A task that does not exist or whose
/// team the actor is not in gives `task_not_found`.
async fn lock_task(
    txn: &impl ConnectionTrait,
    task_id: Uuid,
    actor: Uuid,
) -> Result<task::Model, AppError> {
    let task = task::Entity::find_by_id(task_id)
        .one(txn)
        .await?
        .ok_or(AppError::NotFound("task_not_found"))?;
    teams::require_member(txn, task.team_id, actor)
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::NotFound("task_not_found"),
            other => other,
        })?;

    // Same lock as `insert`, so an insert never races a write for the same
    // end-of-column key; then the task itself, re-read under its lock.
    team::Entity::find_by_id(task.team_id)
        .lock_exclusive()
        .one(txn)
        .await?
        .ok_or(AppError::NotFound("task_not_found"))?;
    let task = task::Entity::find_by_id(task_id)
        .lock_exclusive()
        .one(txn)
        .await?
        .ok_or(AppError::NotFound("task_not_found"))?;
    Ok(task)
}

/// Moves a task to `status`, directly after `after`, or directly before
/// `before` when only that is given (both are optional neighbours in the
/// target column). With neither, it goes to the end of the column. Any team
/// member may move any task (spec §9).
///
/// Neighbours that are not in the task's team and target column, or are the
/// task itself, give `invalid_position`. Neighbours that are out of order
/// (`after` not before `before`) give `stale_position`. A task that
/// does not exist or belongs to a team the caller is not in gives
/// `task_not_found`.
pub async fn move_task(
    db: &(impl TransactionTrait + ConnectionTrait),
    task_id: Uuid,
    actor: Uuid,
    status: TaskStatus,
    after: Option<Uuid>,
    before: Option<Uuid>,
    client: Option<Uuid>,
) -> Result<TaskView, AppError> {
    let txn = db.begin().await?;

    let task = lock_task(&txn, task_id, actor).await?;

    let after_key = neighbour_position(&txn, &task, status, after).await?;
    let before_key = neighbour_position(&txn, &task, status, before).await?;

    // Place the task against the real adjacency of the target column (the task
    // itself excluded), so tasks the client does not see, e.g. hidden by a
    // filter, are never skipped over or collided with.
    let column = || {
        task::Entity::find()
            .filter(task::Column::TeamId.eq(task.team_id))
            .filter(task::Column::Status.eq(status))
            .filter(task::Column::Id.ne(task.id))
    };
    let (lower, upper) = match (after_key, before_key) {
        (Some(after_key), before_key) => {
            if before_key.as_ref().is_some_and(|b| *b <= after_key) {
                return Err(AppError::Conflict("stale_position"));
            }
            let next = column()
                .filter(task::Column::Position.gt(after_key.as_str()))
                .order_by_asc(task::Column::Position)
                .one(&txn)
                .await?
                .map(|t| t.position);
            let upper = match (next, before_key) {
                (Some(next), Some(before_key)) => Some(next.min(before_key)),
                (next, before_key) => next.or(before_key),
            };
            (Some(after_key), upper)
        }
        (None, Some(before_key)) => {
            let previous = column()
                .filter(task::Column::Position.lt(before_key.as_str()))
                .order_by_desc(task::Column::Position)
                .one(&txn)
                .await?
                .map(|t| t.position);
            (previous, Some(before_key))
        }
        (None, None) => {
            let last = column()
                .order_by_desc(task::Column::Position)
                .one(&txn)
                .await?
                .map(|t| t.position);
            (last, None)
        }
    };
    let position = key_between(lower.as_deref(), upper.as_deref())
        .ok_or(AppError::Conflict("stale_position"))?;

    let moved = task::ActiveModel {
        id: Set(task.id),
        status: Set(status),
        position: Set(position),
        updated_at: Set(OffsetDateTime::now_utc()),
        ..Default::default()
    }
    .update(&txn)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => AppError::Conflict("stale_position"),
        _ => AppError::Database(e),
    })?;
    publish(&txn, &moved, "task.moved", client).await?;
    txn.commit().await?;

    views(db, vec![moved])
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("moved task is missing")))
}

/// The position of a neighbour task, which must sit in the moved task's team
/// and in the target column, and must not be the moved task itself.
async fn neighbour_position(
    db: &impl ConnectionTrait,
    task: &task::Model,
    status: TaskStatus,
    neighbour: Option<Uuid>,
) -> Result<Option<String>, AppError> {
    let Some(id) = neighbour else {
        return Ok(None);
    };
    task::Entity::find_by_id(id)
        .filter(task::Column::TeamId.eq(task.team_id))
        .filter(task::Column::Status.eq(status))
        .filter(task::Column::Id.ne(task.id))
        .one(db)
        .await?
        .map(|t| Some(t.position))
        .ok_or(AppError::Invalid("invalid_position"))
}

/// Replaces a task's content. Any team member may edit any task (spec §10);
/// a non-member gets `task_not_found`. The content is sanitized (`invalid_task_content`
/// when blank or invalid) and the mentions are replaced by those of the new
/// content, keeping only team members.
pub async fn edit(
    db: &(impl TransactionTrait + ConnectionTrait),
    task_id: Uuid,
    actor: Uuid,
    raw_content: &str,
    client: Option<Uuid>,
) -> Result<TaskView, AppError> {
    let txn = db.begin().await?;
    let task = lock_task(&txn, task_id, actor).await?;
    let content = rich_text::sanitize(raw_content)?;

    let edited = task::ActiveModel {
        id: Set(task.id),
        content: Set(content.html),
        updated_at: Set(OffsetDateTime::now_utc()),
        ..Default::default()
    }
    .update(&txn)
    .await?;
    task_mention::Entity::delete_many()
        .filter(task_mention::Column::TaskId.eq(task.id))
        .exec(&txn)
        .await?;
    insert_mentions(&txn, task.team_id, task.id, content.mention_ids).await?;
    publish(&txn, &edited, "task.updated", client).await?;
    txn.commit().await?;

    view(db, edited).await
}

/// Deletes a task and its mentions. Any team member may delete any task
/// (spec §10); a non-member gets `task_not_found`.
pub async fn delete(
    db: &(impl TransactionTrait + ConnectionTrait),
    task_id: Uuid,
    actor: Uuid,
    client: Option<Uuid>,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let task = lock_task(&txn, task_id, actor).await?;
    // The mentions go with the task (ON DELETE CASCADE).
    task::Entity::delete_by_id(task.id).exec(&txn).await?;
    publish(&txn, &task, "task.deleted", client).await?;
    txn.commit().await?;
    Ok(())
}

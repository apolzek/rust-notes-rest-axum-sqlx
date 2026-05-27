use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::json;
use sqlx::mysql::MySqlDatabaseError;

use crate::{
    model::{NoteModel, NoteModelResponse},
    schema::{CreateNoteSchema, FilterOptions, UpdateNoteSchema},
    AppState,
};

const MYSQL_ER_DUP_ENTRY: u16 = 1062;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn internal_error(err: impl std::fmt::Debug) -> ApiError {
    tracing::error!(error = ?err, "internal server error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"status": "error", "message": "internal server error"})),
    )
}

fn not_found(id: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"status": "fail", "message": format!("Note with ID: {id} not found")})),
    )
}

pub async fn health_check_handler() -> impl IntoResponse {
    Json(json!({"status": "ok", "message": "API Services"}))
}

pub async fn note_list_handler(
    Query(opts): Query<FilterOptions>,
    State(data): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = opts.limit.unwrap_or(10);
    let offset = (opts.page.unwrap_or(1).saturating_sub(1)) * limit;

    let notes =
        sqlx::query_as::<_, NoteModel>(r#"SELECT * FROM notes ORDER BY id LIMIT ? OFFSET ?"#)
            .bind(limit as i32)
            .bind(offset as i32)
            .fetch_all(&data.db)
            .await
            .map_err(internal_error)?;

    let note_responses = notes
        .iter()
        .map(to_note_response)
        .collect::<Vec<NoteModelResponse>>();

    Ok(Json(json!({
        "status": "ok",
        "count": note_responses.len(),
        "notes": note_responses,
    })))
}

pub async fn create_note_handler(
    State(data): State<Arc<AppState>>,
    Json(body): Json<CreateNoteSchema>,
) -> Result<impl IntoResponse, ApiError> {
    let id = uuid::Uuid::new_v4().to_string();
    let is_published = body.is_published.unwrap_or(false);

    let insert = sqlx::query(
        r#"INSERT INTO notes (id, title, content, is_published) VALUES (?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.title)
    .bind(&body.content)
    .bind(is_published as i8)
    .execute(&data.db)
    .await;

    if let Err(err) = insert {
        if let sqlx::Error::Database(ref db_err) = err {
            if db_err
                .try_downcast_ref::<MySqlDatabaseError>()
                .map(|e| e.number() == MYSQL_ER_DUP_ENTRY)
                .unwrap_or(false)
            {
                return Err((
                    StatusCode::CONFLICT,
                    Json(json!({"status": "error", "message": "Note already exists"})),
                ));
            }
        }
        return Err(internal_error(err));
    }

    let note = sqlx::query_as::<_, NoteModel>(r#"SELECT * FROM notes WHERE id = ?"#)
        .bind(&id)
        .fetch_one(&data.db)
        .await
        .map_err(internal_error)?;

    Ok(Json(json!({
        "status": "success",
        "data": {"note": to_note_response(&note)},
    })))
}

pub async fn get_note_handler(
    Path(id): Path<String>,
    State(data): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    let result = sqlx::query_as::<_, NoteModel>(r#"SELECT * FROM notes WHERE id = ?"#)
        .bind(&id)
        .fetch_one(&data.db)
        .await;

    match result {
        Ok(note) => Ok(Json(json!({
            "status": "success",
            "data": {"note": to_note_response(&note)},
        }))),
        Err(sqlx::Error::RowNotFound) => Err(not_found(&id)),
        Err(e) => Err(internal_error(e)),
    }
}

pub async fn edit_note_handler(
    Path(id): Path<String>,
    State(data): State<Arc<AppState>>,
    Json(body): Json<UpdateNoteSchema>,
) -> Result<impl IntoResponse, ApiError> {
    let note = match sqlx::query_as::<_, NoteModel>(r#"SELECT * FROM notes WHERE id = ?"#)
        .bind(&id)
        .fetch_one(&data.db)
        .await
    {
        Ok(note) => note,
        Err(sqlx::Error::RowNotFound) => return Err(not_found(&id)),
        Err(e) => return Err(internal_error(e)),
    };

    let is_published = body.is_published.unwrap_or(note.is_published != 0);

    let update_result =
        sqlx::query(r#"UPDATE notes SET title = ?, content = ?, is_published = ? WHERE id = ?"#)
            .bind(body.title.unwrap_or(note.title))
            .bind(body.content.unwrap_or(note.content))
            .bind(is_published as i8)
            .bind(&id)
            .execute(&data.db)
            .await
            .map_err(internal_error)?;

    if update_result.rows_affected() == 0 {
        return Err(not_found(&id));
    }

    let updated_note = sqlx::query_as::<_, NoteModel>(r#"SELECT * FROM notes WHERE id = ?"#)
        .bind(&id)
        .fetch_one(&data.db)
        .await
        .map_err(internal_error)?;

    Ok(Json(json!({
        "status": "success",
        "data": {"note": to_note_response(&updated_note)},
    })))
}

pub async fn delete_note_handler(
    Path(id): Path<String>,
    State(data): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    let result = sqlx::query(r#"DELETE FROM notes WHERE id = ?"#)
        .bind(&id)
        .execute(&data.db)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err(not_found(&id));
    }

    Ok(StatusCode::OK)
}

fn to_note_response(note: &NoteModel) -> NoteModelResponse {
    NoteModelResponse {
        id: note.id.clone(),
        title: note.title.clone(),
        content: note.content.clone(),
        is_published: note.is_published != 0,
        created_at: note.created_at.unwrap_or_else(Utc::now),
        updated_at: note.updated_at.unwrap_or_else(Utc::now),
    }
}

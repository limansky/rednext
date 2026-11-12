use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{
    db::{DB, DbError, DbField, DbItem, DbSchema},
    sqlite::SqliteDB,
};

pub async fn create_server(db: Arc<SqliteDB>, port: u16) -> Result<(), std::io::Error> {
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port))
            .await
            .unwrap();
    let app = Router::new()
        .route("/list", get(list_files))
        .route("/file/{name}", get(open_file).delete(delete_file).post(create_file))
        .route("/{name}/items", get(list_items).post(insert_item))
        .route("/{name}/items/done", get(list_done))
        .route("/{name}/items/undone", get(list_undone))
        .route("/{name}/items/random", get(get_random))
        .route("/{name}/items/search", get(search))
        .route("/{name}/items/{id}", get(get_item).delete(delete_item))
        .route("/{name}/items/{id}/done", post(done))
        .route("/{name}/items/{id}/undone", post(undone))
        .with_state(db);

    axum::serve(listener, app).await
}

struct RedError(anyhow::Error);

impl<E> From<E> for RedError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        RedError(err.into())
    }
}

impl IntoResponse for RedError {
    fn into_response(self) -> Response {
        let status = match self.0.downcast_ref::<DbError>() {
            Some(DbError::NotFound(_)) => StatusCode::NOT_FOUND,
            Some(DbError::UnknownField(_)) => StatusCode::BAD_REQUEST,
            Some(DbError::DbAlreadyExists(_)) => StatusCode::CONFLICT,
            None => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.0.to_string()).into_response()
    }
}

async fn list_files(State(db): State<Arc<SqliteDB>>) -> Result<Json<Vec<String>>, RedError> {
    let files = db.list_files().await?;
    Ok(Json(files))
}

async fn open_file(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<DbSchema>, RedError> {
    let file = db.open(&name).await?;
    Ok(Json(file.schema()))
}

async fn delete_file(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<StatusCode, RedError> {
    db.delete(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_file(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
    Json(schema): Json<DbSchema>,
) -> Result<StatusCode, RedError> {
    db.create(&name, schema).await?;
    Ok(StatusCode::CREATED)
}

async fn list_items(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.list_items().await?;
    Ok(Json(items))
}

async fn list_done(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.list_done().await?;
    Ok(Json(items))
}

async fn list_undone(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.list_undone().await?;
    Ok(Json(items))
}

async fn get_item(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
) -> Result<Response, RedError> {
    let mut file = db.open(&name).await?;
    match file.get(id).await? {
        Some(item) => Ok(Json(item).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

async fn get_random(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Response, RedError> {
    let mut file = db.open(&name).await?;
    match file.get_random().await? {
        Some(item) => Ok(Json(item).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

async fn search(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.find(&params.q).await?;
    Ok(Json(items))
}

async fn insert_item(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
    Json(fields): Json<Vec<DbField>>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    file.insert(&fields).await?;
    Ok(StatusCode::CREATED)
}

async fn delete_item(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    file.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn done(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
    Json(time): Json<NaiveDateTime>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    file.done(id, time).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn undone(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    file.undone(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};

use crate::{
    db::{DB, DbError, DbItem, DbSchema},
    sqlite::SqliteDB,
};

pub async fn create_server(db: Arc<SqliteDB>, port: u16) -> Result<(), std::io::Error> {
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port))
            .await
            .unwrap();
    let app = Router::new()
        .route("/list", get(list_files))
        .route("/open/{name}", get(open_file))
        .route("/{name}/items", get(list_items))
        .route("/{name}/items/done", get(list_done))
        .route("/{name}/items/undone", get(list_undone))
        .route("/{name}/items/random", get(get_random))
        .route("/{name}/items/{id}", get(get_item))
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

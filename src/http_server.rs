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
            .await?;
    axum::serve(listener, router(db)).await
}

fn router(db: Arc<SqliteDB>) -> Router {
    Router::new()
        .route("/list", get(list_files))
        .route(
            "/file/{name}",
            get(open_file).delete(delete_file).post(create_file),
        )
        .route("/{name}/items", get(list_items).post(insert_item))
        .route("/{name}/items/done", get(list_done))
        .route("/{name}/items/undone", get(list_undone))
        .route("/{name}/items/random", get(get_random))
        .route("/{name}/items/search", get(search))
        .route("/{name}/items/{id}", get(get_item).delete(delete_item))
        .route("/{name}/items/{id}/done", post(done))
        .route("/{name}/items/{id}/undone", post(undone))
        .with_state(db)
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
            Some(DbError::UnknownField(_) | DbError::MissingField(_)) => StatusCode::BAD_REQUEST,
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
    file.close().await;
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
    db.create(&name, schema).await?.close().await;
    Ok(StatusCode::CREATED)
}

async fn list_items(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.list_items().await;
    file.close().await;
    Ok(Json(items?))
}

async fn list_done(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.list_done().await;
    file.close().await;
    Ok(Json(items?))
}

async fn list_undone(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<DbItem>>, RedError> {
    let file = db.open(&name).await?;
    let items = file.list_undone().await;
    file.close().await;
    Ok(Json(items?))
}

async fn get_item(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
) -> Result<Response, RedError> {
    let mut file = db.open(&name).await?;
    let item = file.get(id).await;
    file.close().await;
    match item? {
        Some(item) => Ok(Json(item).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

async fn get_random(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
) -> Result<Response, RedError> {
    let mut file = db.open(&name).await?;
    let item = file.get_random().await;
    file.close().await;
    match item? {
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
    let items = file.find(&params.q).await;
    file.close().await;
    Ok(Json(items?))
}

async fn insert_item(
    State(db): State<Arc<SqliteDB>>,
    Path(name): Path<String>,
    Json(fields): Json<Vec<DbField>>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    let res = file.insert(&fields).await;
    file.close().await;
    res?;
    Ok(StatusCode::CREATED)
}

async fn delete_item(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    let res = file.delete(id).await;
    file.close().await;
    res?;
    Ok(StatusCode::NO_CONTENT)
}

async fn done(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
    Json(time): Json<NaiveDateTime>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    let res = file.done(id, time).await;
    file.close().await;
    res?;
    Ok(StatusCode::NO_CONTENT)
}

async fn undone(
    State(db): State<Arc<SqliteDB>>,
    Path((name, id)): Path<(String, u32)>,
) -> Result<StatusCode, RedError> {
    let mut file = db.open(&name).await?;
    let res = file.undone(id).await;
    file.close().await;
    res?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{http::StatusCode, response::IntoResponse};
    use chrono::NaiveDate;
    use tempfile::TempDir;

    use super::{RedError, router};
    use crate::{
        db::{DB, DbError, DbField, DbFieldDesc, DbFieldType, DbSchema, DbValue},
        http_client::HttpDB,
        sqlite::SqliteDB,
    };

    /// Starts a server backed by a temporary directory, the directory lives as long as the returned guard
    async fn start_server() -> (HttpDB, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(SqliteDB::new(dir.path()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(db)).await.unwrap() });
        (HttpDB::new(format!("http://{addr}")), dir)
    }

    fn schema() -> DbSchema {
        DbSchema {
            fields: vec![
                DbFieldDesc::new("txt", DbFieldType::Text),
                DbFieldDesc::new("due", DbFieldType::DateTime),
                DbFieldDesc::new("flag", DbFieldType::Boolean),
                DbFieldDesc::new("n", DbFieldType::Number),
            ],
        }
    }

    fn time(day: u32, hour: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2024, 7, day)
            .unwrap()
            .and_hms_opt(hour, 30, 0)
            .unwrap()
    }

    fn item(txt: &str, n: i32) -> Vec<DbField> {
        vec![
            DbField {
                name: "txt".to_string(),
                value: DbValue::Text(txt.to_string()),
            },
            DbField {
                name: "due".to_string(),
                value: DbValue::DateTime(time(1, 12)),
            },
            DbField {
                name: "flag".to_string(),
                value: DbValue::Boolean(true),
            },
            DbField {
                name: "n".to_string(),
                value: DbValue::Number(n),
            },
        ]
    }

    fn db_error(err: anyhow::Error) -> DbError {
        err.downcast::<DbError>().expect("expected DbError")
    }

    #[tokio::test]
    async fn test_file_lifecycle() {
        let (db, _dir) = start_server().await;
        assert!(db.list_files().await.unwrap().is_empty());

        db.create("todo", schema()).await.unwrap();
        assert_eq!(db.list_files().await.unwrap(), vec!["todo".to_string()]);

        let fields = db.open("todo").await.unwrap().schema().fields;
        let names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["txt", "due", "flag", "n"]);
        assert!(fields[1].field_type == DbFieldType::DateTime);

        let err = db.create("todo", schema()).await.err().unwrap();
        assert!(matches!(db_error(err), DbError::DbAlreadyExists(n) if n == "todo"));

        db.delete("todo").await.unwrap();
        assert!(db.list_files().await.unwrap().is_empty());
        let err = db.open("todo").await.err().unwrap();
        assert!(matches!(db_error(err), DbError::NotFound(n) if n == "todo"));
        let err = db.delete("todo").await.unwrap_err();
        assert!(matches!(db_error(err), DbError::NotFound(n) if n == "todo"));
    }

    #[tokio::test]
    async fn test_items_roundtrip() {
        let (db, _dir) = start_server().await;
        let mut file = db.create("todo", schema()).await.unwrap();
        assert!(file.list_items().await.unwrap().is_empty());
        assert!(file.get_random().await.unwrap().is_none());

        file.insert(&item("buy milk", 1)).await.unwrap();
        file.insert(&item("call mom", 2)).await.unwrap();

        let items = file.list_items().await.unwrap();
        assert_eq!(items.len(), 2);
        let values: Vec<_> = items[0].fields.iter().map(|f| &f.value).collect();
        assert_eq!(
            values,
            vec![
                &DbValue::Text("buy milk".to_string()),
                &DbValue::DateTime(time(1, 12)),
                &DbValue::Boolean(true),
                &DbValue::Number(1),
            ]
        );

        let second = file.get(2).await.unwrap().unwrap();
        assert_eq!(second.fields[3].value, DbValue::Number(2));
        assert!(file.get(42).await.unwrap().is_none());

        // the query must survive URL encoding
        let found = file.find("buy milk").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, 1);
        assert_eq!(file.find("a&b=c").await.unwrap().len(), 0);

        file.done(1, time(3, 9)).await.unwrap();
        let done = file.list_done().await.unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].completed_at, Some(time(3, 9)));
        let undone = file.list_undone().await.unwrap();
        assert_eq!(undone.iter().map(|i| i.id).collect::<Vec<_>>(), vec![2]);
        assert_eq!(file.get_random().await.unwrap().unwrap().id, 2);

        file.undone(1).await.unwrap();
        assert!(file.list_done().await.unwrap().is_empty());

        file.delete(1).await.unwrap();
        let ids: Vec<_> = file
            .list_items()
            .await
            .unwrap()
            .iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids, vec![2]);
    }

    #[tokio::test]
    async fn test_item_errors() {
        let (db, _dir) = start_server().await;
        let mut file = db.create("todo", schema()).await.unwrap();

        let unknown = vec![DbField {
            name: "nope".to_string(),
            value: DbValue::Number(1),
        }];
        // the server's message reaches the client
        let err = file.insert(&unknown).await.unwrap_err();
        assert!(err.to_string().contains("Unknown field nope"), "{err}");
        let partial = &item("x", 1)[..2];
        let err = file.insert(partial).await.unwrap_err();
        assert!(err.to_string().contains("Missing field flag"), "{err}");
        assert!(file.list_items().await.unwrap().is_empty());

        assert!(file.done(42, time(1, 0)).await.is_err());
        assert!(file.undone(42).await.is_err());
    }

    #[tokio::test]
    async fn test_items_of_deleted_file() {
        let (db, _dir) = start_server().await;
        let mut file = db.create("todo", schema()).await.unwrap();
        db.delete("todo").await.unwrap();

        let err = file.list_items().await.err().unwrap();
        assert!(matches!(db_error(err), DbError::NotFound(n) if n == "todo"));
        let err = file.find("x").await.err().unwrap();
        assert!(matches!(db_error(err), DbError::NotFound(_)));
        let err = file.insert(&item("x", 1)).await.unwrap_err();
        assert!(matches!(db_error(err), DbError::NotFound(_)));
    }

    #[test]
    fn test_error_status() {
        let status = |e: anyhow::Error| RedError(e).into_response().status();
        assert_eq!(
            status(DbError::NotFound("x".into()).into()),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(DbError::UnknownField("x".into()).into()),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status(DbError::MissingField("x".into()).into()),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status(DbError::DbAlreadyExists("x".into()).into()),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(anyhow::anyhow!("boom")),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        // DbError wrapped in context must still be recognized
        let wrapped = anyhow::Error::from(DbError::NotFound("x".into())).context("Cannot open");
        assert_eq!(status(wrapped), StatusCode::NOT_FOUND);
    }
}

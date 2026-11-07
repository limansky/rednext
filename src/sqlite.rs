use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    result,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use sqlx::{
    Connection, Row, SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
};

use crate::db::{
    DB, DBFile, DbError, DbField, DbFieldDesc, DbFieldType, DbItem, DbSchema, DbValue,
};

pub struct SqliteDB {
    path: PathBuf,
}

impl SqliteDB {
    pub fn new(db_path: &Path) -> Self {
        SqliteDB {
            path: db_path.to_path_buf(),
        }
    }

    async fn read_schema(conn: &mut SqliteConnection) -> sqlx::Result<DbSchema> {
        let rows = sqlx::query("SELECT name, datatype FROM schema ORDER BY idx")
            .fetch_all(conn)
            .await?;
        let fields = rows
            .iter()
            .map(|row| {
                let field_name: String = row.try_get(0)?;
                let field_type: DbFieldType = row.try_get(1)?;
                Ok(DbFieldDesc {
                    name: field_name,
                    field_type,
                })
            })
            .collect::<sqlx::Result<Vec<_>>>()?;
        Ok(DbSchema { fields })
    }

    async fn write_schema(conn: &mut SqliteConnection, schema: &DbSchema) -> sqlx::Result<()> {
        let mut tx = conn.begin().await?;

        sqlx::query(
            "CREATE TABLE schema (
            name TEXT PRIMARY KEY,
            datatype TEXT NOT NULL,
            idx INTEGER NOT NULL
          )",
        )
        .execute(&mut *tx)
        .await?;

        for (idx, field) in schema.fields.iter().enumerate() {
            sqlx::query("INSERT INTO schema (name, datatype, idx) VALUES (?1, ?2, ?3)")
                .bind(field.name.clone())
                .bind(field.field_type.to_string())
                .bind(idx as u32)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn create_items_table(
        conn: &mut SqliteConnection,
        schema: &DbSchema,
    ) -> sqlx::Result<()> {
        let field_defs = schema
            .fields
            .iter()
            .map(|f| {
                let sql_type = match f.field_type {
                    DbFieldType::Text => "TEXT",
                    DbFieldType::Number => "NUMBER",
                    DbFieldType::Boolean => "BOOLEAN",
                    DbFieldType::DateTime => "TIMESTAMP",
                };
                format!("\"{}\" {}", f.name.to_lowercase(), sql_type)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "CREATE TABLE items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            {field_defs},
            done_at TIMESTAMP
          )"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql)).execute(conn).await?;

        Ok(())
        // .context("Cannot create items table")?
    }
}

struct SqliteFile {
    pool: SqlitePool,
    schema: DbSchema,
}

async fn connect(path: &Path, create: bool) -> sqlx::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create);
    SqlitePoolOptions::new().connect_with(options).await
}

#[async_trait]
impl DB for SqliteDB {
    async fn list_files(&self) -> Result<Vec<String>> {
        if self.path.exists() {
            if self.path.is_dir() {
                fs::read_dir(&self.path)
                    .map(|rd| {
                        rd.filter_map(result::Result::ok)
                            .filter(|d| d.path().extension() == Some(OsStr::new("db")))
                            .flat_map(|d| {
                                d.path()
                                    .file_stem()
                                    .and_then(|x| x.to_str().map(|s| s.to_string()))
                            })
                            .collect()
                    })
                    .context("Cannot read databases")
            } else {
                Err(anyhow!("Invalid DB path {:?}", self.path))
            }
        } else {
            Ok(vec![])
        }
    }

    async fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>> {
        if !self.path.exists() {
            fs::create_dir_all(&self.path).context("Cannot create directory")?;
        }

        if !self.path.is_dir() {
            return Err(anyhow!("{:?} is not a directory", self.path));
        }

        let file_path = self.path.join(format!("{}.db", name));

        if file_path.exists() {
            return Err(anyhow!("Database file {:?} already exists", file_path));
        }

        let pool = connect(&file_path, true)
            .await
            .context("Cannot create DB file")?;
        let mut conn = pool.acquire().await?;
        Self::write_schema(&mut conn, &schema)
            .await
            .context("Cannot write schema")?;
        Self::create_items_table(&mut conn, &schema)
            .await
            .context("Cannot create items table")?;
        drop(conn);
        Ok(Box::new(SqliteFile { pool, schema }))
    }

    async fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        if !path.exists() {
            return Err(DbError::NotFound(name.to_string()).into());
        }
        let pool = connect(&path, false).await.context("Cannot open DB")?;
        let mut conn = pool.acquire().await?;
        let schema = Self::read_schema(&mut conn)
            .await
            .context("Cannot read schema")?;
        drop(conn);

        Ok(Box::new(SqliteFile { pool, schema }))
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        fs::remove_file(path).context("Cannot delete file")
    }
}

impl SqliteFile {
    fn to_db_item(&self, row: &SqliteRow) -> sqlx::Result<DbItem> {
        let fields = self
            .schema
            .fields
            .iter()
            .map(|f| {
                let name = f.name.to_lowercase();
                let value = match f.field_type {
                    DbFieldType::Text => DbValue::Text(row.try_get(&*name)?),
                    DbFieldType::Number => DbValue::Number(row.try_get(&*name)?),
                    DbFieldType::Boolean => DbValue::Boolean(row.try_get(&*name)?),
                    DbFieldType::DateTime => DbValue::DateTime(row.try_get(&*name)?),
                };
                Ok(DbField {
                    name: f.name.clone(),
                    value,
                })
            })
            .collect::<sqlx::Result<Vec<_>>>()?;
        Ok(DbItem {
            id: row.try_get("id")?,
            fields,
            completed_at: row.try_get("done_at")?,
        })
    }

    async fn select_items(
        &self,
        filter: Option<&str>,
        params: &[&str],
        order_by: Option<&str>,
    ) -> Result<Vec<DbItem>> {
        let ord = order_by.unwrap_or("id");
        let base_query = self.base_select();
        let mut q = filter
            .map(|c| format!("{base_query} WHERE {c}"))
            .unwrap_or(base_query);
        q.push_str(&format!(" ORDER BY {ord}"));
        let mut query = sqlx::query(sqlx::AssertSqlSafe(q));
        for p in params {
            query = query.bind(*p);
        }
        query
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|row| self.to_db_item(row))
            .collect::<sqlx::Result<Vec<_>>>()
            .context("Item query error")
    }

    fn base_select(&self) -> String {
        let fields = self
            .schema
            .fields
            .iter()
            .map(|f| format!("\"{}\"", f.name.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        format!("SELECT id, {fields}, done_at FROM items")
    }
}

#[async_trait]
impl DBFile for SqliteFile {
    fn schema(&self) -> DbSchema {
        self.schema.clone()
    }

    async fn insert(&mut self, fields: &[DbField]) -> Result<()> {
        let field_names = fields
            .iter()
            .map(|f| format!("\"{}\"", f.name.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=fields.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!("INSERT INTO items ({field_names}) VALUES({placeholders})");
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));

        for f in fields {
            query = match &f.value {
                DbValue::Text(s) => query.bind(s.as_str()),
                DbValue::Number(n) => query.bind(*n),
                DbValue::Boolean(b) => query.bind(*b),
                DbValue::DateTime(dt) => query.bind(*dt),
            };
        }
        query
            .execute(&self.pool)
            .await
            .context("Cannot insert item")?;
        Ok(())
    }

    async fn delete(&mut self, id: u32) -> Result<()> {
        sqlx::query("DELETE FROM items WHERE id=?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Cannot delete item")?;
        Ok(())
    }

    async fn list_items(&self) -> Result<Vec<DbItem>> {
        self.select_items(None, &[], None).await
    }

    async fn list_done(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NOT NULL"), &[], Some("done_at"))
            .await
    }

    async fn list_undone(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NULL"), &[], None).await
    }

    async fn find(&self, item_name: &str) -> Result<Vec<DbItem>> {
        let pattern = format!("%{item_name}%");
        let fields = self.schema.fields.iter().flat_map(|f| {
            if f.field_type == DbFieldType::Text {
                Some(format!("\"{}\" LIKE ?1", f.name))
            } else {
                None
            }
        });
        let filter = fields.collect::<Vec<_>>().join(" OR ");
        self.select_items(Some(filter.as_str()), &[&pattern], None)
            .await
    }

    async fn get_random(&mut self) -> Result<Option<DbItem>> {
        let base_query = self.base_select();
        let sql = format!(
            "{base_query}
                     WHERE done_at IS NULL
                     ORDER BY random()
                     LIMIT 1"
        );
        let item = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_optional(&self.pool)
            .await?;
        Ok(item.map(|row| self.to_db_item(&row)).transpose()?)
    }

    async fn get(&mut self, id: u32) -> Result<Option<DbItem>> {
        let sql = format!("{} WHERE id=?1", self.base_select());
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| self.to_db_item(&row))
            .transpose()
            .context("Cannot get item")
    }

    async fn done(&mut self, id: u32, time: chrono::NaiveDateTime) -> Result<()> {
        let count = sqlx::query("UPDATE items SET done_at=?1 WHERE id =?2")
            .bind(time)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if count == 1 {
            Ok(())
        } else {
            Err(anyhow!("Expect exact one item, but got {}", count))
        }
    }

    async fn undone(&mut self, id: u32) -> Result<()> {
        let count = sqlx::query("UPDATE items SET done_at=NULL WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if count == 1 {
            Ok(())
        } else {
            Err(anyhow!("Item with id {id} is not found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use sqlx::sqlite::SqlitePoolOptions;

    use crate::{
        db::{DB, DBFile, DbField, DbFieldDesc, DbFieldType, DbSchema, DbValue},
        sqlite::{SqliteDB, SqliteFile},
    };

    async fn create_file() -> SqliteFile {
        // A single long-lived connection, otherwise each pooled connection gets its own in-memory DB
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE items(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            txt TEXT,
            due TIMESTAMP,
            n NUMBER,
            bool BOOLEAN,
            done_at TIMESTAMP,
            comment TEXT
          )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let schema = DbSchema {
            fields: vec![
                DbFieldDesc::new("txt", DbFieldType::Text),
                DbFieldDesc::new("due", DbFieldType::DateTime),
                DbFieldDesc::new("bool", DbFieldType::Boolean),
                DbFieldDesc::new("n", DbFieldType::Number),
            ],
        };
        SqliteFile { pool, schema }
    }

    #[tokio::test]
    async fn test_insert() {
        let mut file = create_file().await;
        assert!(file.list_items().await.unwrap().is_empty());
        file.insert(&[
            DbField {
                name: "txt".to_string(),
                value: DbValue::Text("task 1".to_string()),
            },
            DbField {
                name: "due".to_string(),
                value: DbValue::DateTime(
                    chrono::NaiveDate::from_ymd_opt(2024, 7, 1)
                        .unwrap()
                        .and_hms_opt(1, 20, 0)
                        .unwrap(),
                ),
            },
            DbField {
                name: "bool".to_string(),
                value: DbValue::Boolean(true),
            },
            DbField {
                name: "n".to_string(),
                value: DbValue::Number(42),
            },
        ])
        .await
        .unwrap();

        let items = file.list_items().await.unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.id, 1);
        assert_eq!(item.fields.len(), 4);
        assert_eq!(item.fields[0].name, "txt");
        assert_eq!(item.fields[0].value, DbValue::Text("task 1".to_string()));
        assert_eq!(item.fields[1].name, "due");
        assert_eq!(
            item.fields[1].value,
            DbValue::DateTime(
                NaiveDate::from_ymd_opt(2024, 7, 1)
                    .unwrap()
                    .and_hms_opt(1, 20, 0)
                    .unwrap(),
            )
        );
        assert_eq!(item.fields[2].name, "bool");
        assert_eq!(item.fields[2].value, DbValue::Boolean(true));
        assert_eq!(item.fields[3].name, "n");
        assert_eq!(item.fields[3].value, DbValue::Number(42));

        assert_eq!(file.find("task").await.unwrap().len(), 1);
        file.done(
            1,
            NaiveDate::from_ymd_opt(2024, 7, 2)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(file.list_done().await.unwrap().len(), 1);
        assert!(file.list_undone().await.unwrap().is_empty());
        assert!(file.get_random().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_create_and_open() {
        let dir = std::env::temp_dir().join(format!("rednext-test-{}", std::process::id()));
        let db = SqliteDB::new(&dir);
        let schema = DbSchema {
            fields: vec![
                DbFieldDesc::new("txt", DbFieldType::Text),
                DbFieldDesc::new("n", DbFieldType::Number),
            ],
        };
        db.create("test", schema).await.unwrap();
        assert_eq!(db.list_files().await.unwrap(), vec!["test".to_string()]);
        let file = db.open("test").await.unwrap();
        let fields = file.schema().fields;
        assert_eq!(fields.len(), 2);
        assert!(fields[0].name == "txt" && fields[0].field_type == DbFieldType::Text);
        assert!(fields[1].name == "n" && fields[1].field_type == DbFieldType::Number);
        drop(file);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

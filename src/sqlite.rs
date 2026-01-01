use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    result,
};

use anyhow::{anyhow, Context, Result};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    Connection, OptionalExtension, Params, Row,
};

use crate::db::{DBFile, DbField, DbFieldDesc, DbFieldType, DbItem, DbSchema, DbValue, DB};

pub struct SqliteDB {
    path: PathBuf,
}

impl SqliteDB {
    pub fn new(db_path: &Path) -> Self {
        SqliteDB {
            path: db_path.to_path_buf(),
        }
    }

    fn read_schema(conn: &Connection) -> rusqlite::Result<DbSchema> {
        let mut stmt = conn.prepare("SELECT name, datatype FROM schema ORDER BY idx")?;
        let fields = stmt
            .query_map([], |row| {
                let field_name: String = row.get(0)?;
                let field_type: DbFieldType = row.get(1)?;
                Ok(DbFieldDesc {
                    name: field_name,
                    field_type,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(DbSchema { fields })
    }
}

struct SqliteFile {
    connection: Connection,
    schema: DbSchema,
}

impl DB for SqliteDB {
    fn list_files(&self) -> Result<Vec<String>> {
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

    fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>> {
        if !self.path.exists() {
            fs::create_dir_all(&self.path).context("Cannot create directory")?;
        }

        if !self.path.is_dir() {
            return Err(anyhow!("{:?} is not a directory", self.path));
        }
        unimplemented!("note imp");
    }

    fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        let conn = Connection::open(path).context("Cannot open DB")?;
        let schema = Self::read_schema(&conn).context("Cannot read schema")?;

        Ok(Box::new(SqliteFile {
            connection: conn,
            schema,
        }))
    }

    fn delete(&self, name: &str) -> Result<()> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        fs::remove_file(path).context("Cannot delete file")
    }
}

impl SqliteFile {
    fn to_db_item(&self, row: &Row) -> rusqlite::Result<DbItem> {
        let fields: rusqlite::Result<Vec<DbField>> = self
            .schema
            .fields
            .iter()
            .map(|f| {
                let value = match f.field_type {
                    DbFieldType::Text => DbValue::Text(row.get(f.name.as_str())?),
                    DbFieldType::Number => DbValue::Number(row.get(f.name.as_str())?),
                    DbFieldType::Boolean => DbValue::Boolean(row.get(f.name.as_str())?),
                    DbFieldType::DateTime => DbValue::DateTime(row.get(f.name.as_str())?),
                };
                Ok(DbField {
                    name: f.name.clone(),
                    value,
                })
            })
            .collect();
        Ok(DbItem {
            id: row.get("id")?,
            fields: fields?,
            completed_at: row.get("done_at")?,
        })
    }

    fn select_items<P: Params>(
        &self,
        filter: Option<&str>,
        params: P,
        order_by: Option<&str>,
    ) -> Result<Vec<DbItem>> {
        let ord = order_by.unwrap_or("id");
        let base_query = self.base_select();
        let mut q = filter
            .map(|c| format!("{base_query} WHERE {c}"))
            .unwrap_or(base_query);
        q.push_str(&format!(" ORDER BY {ord}"));
        let mut stmt = self.connection.prepare(&q)?;
        let iter = stmt.query_map(params, |row| self.to_db_item(row))?;
        iter.collect::<rusqlite::Result<Vec<_>>>()
            .context("Item query error")
    }

    fn base_select(&self) -> String {
        let fields = self
            .schema
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("SELECT id, {fields}, done_at FROM items")
    }
}

impl DBFile for SqliteFile {
    fn schema(&self) -> Result<DbSchema> {
        Ok(self.schema.clone())
    }

    fn insert(&self, item_name: &str) -> Result<()> {
        self.connection
            .execute("INSERT INTO items (name) VALUES(?1)", params![item_name])
            .context("Cannot insert item")?;
        Ok(())
    }

    fn delete(&self, id: u32) -> Result<()> {
        self.connection
            .execute("DELETE FROM items WHERE id=?1", params![id])
            .context("Cannot delete item")?;
        Ok(())
    }

    fn list_items(&self) -> Result<Vec<DbItem>> {
        self.select_items(None, [], None)
    }

    fn list_done(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NOT NULL"), [], Some("done_at"))
    }

    fn list_undone(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NULL"), [], None)
    }

    fn find(&self, item_name: &str) -> Result<Vec<DbItem>> {
        let pattern = format!("%{item_name}%");
        self.select_items(Some("name LIKE ?1"), params![pattern], None)
    }

    fn get_random(&self) -> Result<Option<DbItem>> {
        let base_query = self.base_select();
        self.connection
            .query_one(
                format!(
                    "{base_query}
                     WHERE done_at IS NULL
                     ORDER BY random()
                     LIMIT 1"
                )
                .as_str(),
                [],
                |row| self.to_db_item(row),
            )
            .optional()
            .context("Query error")
    }

    fn get(&self, id: u32) -> Result<Option<DbItem>> {
        let base_query = self.base_select();
        self.connection
            .query_one(
                format!("{base_query} WHERE id=?1").as_str(),
                params![id],
                |row| self.to_db_item(row),
            )
            .optional()
            .context("Query error")
    }

    fn done(&self, id: u32, time: chrono::NaiveDateTime) -> Result<()> {
        let count = self
            .connection
            .execute(
                "UPDATE items SET done_at=?1 WHERE id =?2",
                params![time, id],
            )
            .context("Cannot update item ")?;
        if count == 1 {
            Ok(())
        } else {
            Err(anyhow!("Expect exact one item, but got {}", count))
        }
    }

    fn undone(&self, id: u32) -> Result<()> {
        let count = self
            .connection
            .execute("UPDATE items SET done_at=NULL WHERE id =?1", params![id])
            .context("Cannot update item")?;

        if count == 1 {
            Ok(())
        } else {
            Err(anyhow!("Item with id {id} is not found"))
        }
    }
}

impl FromSql for DbFieldType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|e| FromSqlError::Other(Box::new(e)))
    }
}

use std::{ffi::OsStr, fs, path::PathBuf, result};

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::db::{DB, DBFile, DbItem};

pub struct SqliteDB {
    path: PathBuf,
}

impl SqliteDB {
    pub fn new(db_path: &PathBuf) -> Self {
        SqliteDB {
            path: db_path.clone(),
        }
    }
}

struct SqliteFile {
    connection: Connection,
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

    fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        if !self.path.exists() {
            fs::create_dir_all(&self.path).context("Cannot create directory")?;
        }

        if !self.path.is_dir() {
            return Err(anyhow!("{:?} is not a directory", self.path));
        }

        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        let conn = Connection::open(path).context("Cannot open DB")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                done_at TIMESTAMP,
                comment TEXT
        )",
            (),
        )
        .context("Cannot initialize DB")?;
        Ok(Box::new(SqliteFile { connection: conn }))
    }

    fn delete(&self, name: &str) -> Result<()> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        fs::remove_file(path).context("Cannot delete file")
    }
}

impl SqliteFile {
    fn to_db_item(row: &Row) -> rusqlite::Result<DbItem> {
        Ok(DbItem {
            id: row.get("id")?,
            name: row.get("name")?,
            completed_at: row.get("done_at")?,
        })
    }

    fn select_items(
        &self,
        filter: Option<&str>,
        order_by: Option<&str>,
    ) -> rusqlite::Result<Vec<DbItem>> {
        let ord = order_by.unwrap_or("id");
        let base_query = "SELECT id, name, done_at FROM items".to_string();
        let mut q = filter
            .map(|c| format!("{base_query}  WHERE {c}"))
            .unwrap_or(base_query);
        q.push_str(&format!(" ORDER BY {ord}"));
        let mut stmt = self.connection.prepare(&q)?;
        let iter = stmt.query_map([], Self::to_db_item)?;
        iter.collect()
    }
}

impl DBFile for SqliteFile {
    fn insert(&self, item_name: &str) -> Result<()> {
        self.connection
            .execute("INSERT INTO items (name) VALUES(?1)", params![item_name])
            .context("Cannot insert item")?;
        Ok(())
    }

    fn delete(&self, id: u64) -> Result<()> {
        self.connection
            .execute("DELETE FROM items WHERE id=?1", params![id])
            .context("Cannot delete item")?;
        Ok(())
    }

    fn list_items(&self) -> Result<Vec<DbItem>> {
        self.select_items(None, None).context("Query error")
    }

    fn list_done(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NOT NULL"), Some("done_at"))
            .context("Query error")
    }

    fn list_undone(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NULL"), None)
            .context("Query error")
    }

    fn get_random(&self) -> Result<Option<DbItem>> {
        self.connection
            .query_one(
                "SELECT id, name, done_at
                   FROM items
                   WHERE done_at IS NULL
                   ORDER BY random()
                   LIMIT 1",
                [],
                Self::to_db_item,
            )
            .optional()
            .context("Query error")
    }

    fn get(&self, id: u64) -> Result<Option<DbItem>> {
        self.connection
            .query_one(
                "SELECT id, name, done_at FROM items WHERE id=?1",
                params![id],
                Self::to_db_item,
            )
            .optional()
            .context("Query error")
    }

    fn done(&self, id: u64, time: chrono::NaiveDateTime) -> Result<()> {
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

    fn undone(&self, id: u64) -> Result<()> {
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

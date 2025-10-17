use std::{ffi::OsStr, fs, path::PathBuf, result};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::db::{DB, DBFile, DbItem, Problem, Result};

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
                    .map_err(|e| Problem::IOError(format!("Cannot read databases {}", e)))
            } else {
                Err(Problem::IOError(format!("Invalid DB path {:?}", self.path)))
            }
        } else {
            Ok(vec![])
        }
    }

    fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        if !self.path.exists() {
            fs::create_dir_all(&self.path)
                .map_err(|e| Problem::IOError(format!("Cannot create directory, {}", e)))?;
        }

        if !self.path.is_dir() {
            return Err(Problem::IOError(format!(
                "{:?} is not a directory",
                self.path
            )));
        }

        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        let conn = Connection::open(path)
            .map_err(|e| Problem::DBError(format!("Cannot open DB, {}", e)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                done_at TIMESTAMP,
                comment TEXT
        )",
            (),
        )
        .map_err(|e| Problem::DBError(format!("Cannot initialize DB, {}", e)))?;
        Ok(Box::new(SqliteFile { connection: conn }))
    }

    fn delete(&self, name: &str) -> Result<()> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        fs::remove_file(path).map_err(|e| Problem::IOError(format!("Cannot delete file, {e}")))
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

    fn select_items(&self) -> rusqlite::Result<Vec<DbItem>> {
        let mut stmt = self
            .connection
            .prepare("SELECT id, name, done_at FROM items")?;
        let iter = stmt.query_map([], Self::to_db_item)?;
        iter.collect()
    }

    fn select_random_undone(&self) -> rusqlite::Result<Option<DbItem>> {
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
    }
}

impl DBFile for SqliteFile {
    fn insert(&self, item_name: &str) -> Result<()> {
        self.connection
            .execute("INSERT INTO items (name) VALUES(?1)", params![item_name])
            .map_err(|e| Problem::DBError(format!("Cannot insert item {e}")))?;
        Ok(())
    }

    fn list_items(&self) -> Result<Vec<DbItem>> {
        self.select_items()
            .map_err(|e| Problem::DBError(format!("Query error {e}")))
    }

    fn get_random(&self) -> Result<Option<DbItem>> {
        self.select_random_undone()
            .map_err(|e| Problem::DBError(format!("Query error {e}")))
    }

    fn done(&self, id: u64, time: chrono::NaiveDateTime) -> Result<()> {
        let count = self
            .connection
            .execute(
                "UPDATE items SET done_at=?1 WHERE id =?2",
                params![time, id],
            )
            .map_err(|e| Problem::DBError(format!("Cannot update item, {e}")))?;
        if count == 1 {
            Ok(())
        } else {
            Err(Problem::DBError(format!(
                "Expect exact one item, but got {}",
                count
            )))
        }
    }
}

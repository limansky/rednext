use std::{ffi::OsStr, fs, path::PathBuf, result};

use rusqlite::Connection;

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

    fn open(&self, name: &str) -> Result<Box<dyn crate::db::DBFile>> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        let conn = Connection::open(path)
            .map_err(|e| Problem::DBError(format!("Cannot open DB {}", e)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items(
                name TEXT,
                done_at TIMESTAMP,
                comment TEXT
        )",
            (),
        )
        .map_err(|e| Problem::DBError(format!("Cannot initialize DB {}", e)))?;
        Ok(Box::new(SqliteFile { connection: conn }))
    }
}

impl SqliteFile {
    fn select_items(&self) -> rusqlite::Result<Vec<DbItem>> {
        let mut stmt = self.connection.prepare("SELECT name FROM items")?;
        let iter = stmt.query_map([], |row| Ok(DbItem { name: row.get(0)? }))?;
        iter.collect()
    }
}

impl DBFile for SqliteFile {
    fn list_items(&self) -> Result<Vec<crate::db::DbItem>> {
        self.select_items()
            .map_err(|e| Problem::DBError(format!("Query error {e}")))
    }
}

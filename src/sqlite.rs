use std::{ffi::OsStr, fs, path::PathBuf};

use crate::db::{DB, Problem};

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

impl DB for SqliteDB {
    fn list_files(&self) -> Result<Vec<String>, crate::db::Problem> {
        if self.path.exists() {
            if self.path.is_dir() {
                fs::read_dir(&self.path)
                    .map(|rd| {
                        rd.filter_map(Result::ok)
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
}

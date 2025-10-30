use anyhow::Result;
use chrono::NaiveDateTime;

pub trait DB {
    fn list_files(&self) -> Result<Vec<String>>;
    fn open(&self, name: &str) -> Result<Box<dyn DBFile>>;
    fn delete(&self, name: &str) -> Result<()>;
}

pub trait DBFile {
    fn list_items(&self) -> Result<Vec<DbItem>>;
    fn list_done(&self) -> Result<Vec<DbItem>>;
    fn list_undone(&self) -> Result<Vec<DbItem>>;
    fn insert(&self, item_name: &str) -> Result<()>;
    fn delete(&self, id: u64) -> Result<()>;
    fn get(&self, id: u64) -> Result<Option<DbItem>>;
    fn get_random(&self) -> Result<Option<DbItem>>;
    fn done(&self, id: u64, time: NaiveDateTime) -> Result<()>;
    fn undone(&self, id: u64) -> Result<()>;
}

pub struct DbItem {
    pub id: u64,
    pub name: String,
    pub completed_at: Option<NaiveDateTime>,
}

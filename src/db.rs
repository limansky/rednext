use std::fmt::{Display, Formatter};

use anyhow::Result;
use chrono::NaiveDateTime;
use strum::{Display, EnumString};

pub trait DB {
    fn list_files(&self) -> Result<Vec<String>>;
    fn open(&self, name: &str) -> Result<Box<dyn DBFile>>;
    fn delete(&self, name: &str) -> Result<()>;
    fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>>;
}

pub trait DBFile {
    fn schema(&self) -> Result<DbSchema>;
    fn list_items(&self) -> Result<Vec<DbItem>>;
    fn list_done(&self) -> Result<Vec<DbItem>>;
    fn list_undone(&self) -> Result<Vec<DbItem>>;
    fn insert(&self, fields: &[DbField]) -> Result<()>;
    fn delete(&self, id: u32) -> Result<()>;
    fn get(&self, id: u32) -> Result<Option<DbItem>>;
    fn get_random(&self) -> Result<Option<DbItem>>;
    fn done(&self, id: u32, time: NaiveDateTime) -> Result<()>;
    fn undone(&self, id: u32) -> Result<()>;
    fn find(&self, item_name: &str) -> Result<Vec<DbItem>>;
}

#[derive(Clone)]
pub struct DbSchema {
    pub fields: Vec<DbFieldDesc>,
}

#[derive(Clone)]
pub struct DbFieldDesc {
    pub name: String,
    pub field_type: DbFieldType,
}

impl DbFieldDesc  {
    pub fn new(name: &str, field_type: DbFieldType) -> Self {
        Self {
            name: name.to_string(),
            field_type,
        }
    }
}

#[derive(Clone, EnumString, Display, PartialEq, Eq)]
pub enum DbFieldType {
    Text,
    Number,
    Boolean,
    DateTime,
}

pub struct DbField {
    pub name: String,
    pub value: DbValue,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DbValue {
    Text(String),
    Number(i32),
    Boolean(bool),
    DateTime(NaiveDateTime),
}

pub struct DbItem {
    pub id: u32,
    pub fields: Vec<DbField>,
    pub completed_at: Option<NaiveDateTime>,
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DbValue::Text(s) => write!(f, "{}", s),
            DbValue::Number(n) => write!(f, "{}", n),
            DbValue::Boolean(b) => write!(f, "{}", b),
            DbValue::DateTime(dt) => write!(f, "{}", dt),
        }
    }
}

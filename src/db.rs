use std::fmt::{Display, Formatter};

use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[async_trait]
pub trait DB {
    async fn list_files(&self) -> Result<Vec<String>>;
    async fn open(&self, name: &str) -> Result<Box<dyn DBFile>>;
    async fn delete(&self, name: &str) -> Result<()>;
    async fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>>;
}

#[async_trait]
pub trait DBFile: Send + Sync {
    fn schema(&self) -> DbSchema;
    async fn list_items(&self) -> Result<Vec<DbItem>>;
    async fn list_done(&self) -> Result<Vec<DbItem>>;
    async fn list_undone(&self) -> Result<Vec<DbItem>>;
    async fn insert(&mut self, fields: &[DbField]) -> Result<()>;
    async fn delete(&mut self, id: u32) -> Result<()>;
    async fn get(&mut self, id: u32) -> Result<Option<DbItem>>;
    async fn get_random(&mut self) -> Result<Option<DbItem>>;
    async fn done(&mut self, id: u32, time: NaiveDateTime) -> Result<()>;
    async fn undone(&mut self, id: u32) -> Result<()>;
    async fn find(&self, item_name: &str) -> Result<Vec<DbItem>>;
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DbSchema {
    pub fields: Vec<DbFieldDesc>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DbFieldDesc {
    pub name: String,
    pub field_type: DbFieldType,
}

impl DbFieldDesc {
    pub fn new(name: &str, field_type: DbFieldType) -> Self {
        Self {
            name: name.to_string(),
            field_type,
        }
    }
}

#[derive(Clone, EnumString, Deserialize, Serialize, Display, PartialEq, Eq, sqlx::Type)]
pub enum DbFieldType {
    Text,
    Number,
    Boolean,
    DateTime,
}

#[derive(Deserialize, Serialize)]
pub struct DbField {
    pub name: String,
    pub value: DbValue,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum DbValue {
    Text(String),
    Number(i32),
    Boolean(bool),
    DateTime(NaiveDateTime),
}

#[derive(Deserialize, Serialize)]
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

#[derive(Debug)]
pub enum DbError {
    NotFound(String),
}

impl Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::NotFound(name) => write!(f, "Database {} not found", name),
        }
    }
}

impl std::error::Error for DbError {}

use anyhow::Result;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};

use crate::db::{DB, DBFile, DbError, DbField, DbItem, DbSchema};

pub struct HttpDB {
    url: String,
}

struct HttpDBFile {
    url: String,
    schema: DbSchema,
    client: Client,
}

impl HttpDB {
    pub fn new(url: String) -> Self {
        HttpDB { url }
    }
}

#[async_trait]
impl DB for HttpDB {
    async fn list_files(&self) -> Result<Vec<String>> {
        let mut url = self.url.clone();
        url.push_str("/list");
        let res: Vec<String> = reqwest::get(url).await?.json().await?;
        Ok(res)
    }

    async fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        let mut url = self.url.clone();
        url.push_str("/open/");
        url.push_str(name);
        let res = reqwest::get(url).await?;
        if res.status() == StatusCode::NOT_FOUND {
            return Err(DbError::NotFound(name.to_string()).into());
        }
        let schema = res.error_for_status()?.json().await?;
        let url = format!("{}/{}", self.url, name);
        Ok(Box::new(HttpDBFile {
            url,
            schema,
            client: Client::new(),
        }))
    }

    async fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>> {
        let mut url = self.url.clone();
        url.push_str("/create/");
        url.push_str(name);
        let client = Client::new();
        client.put(url).json(&schema).send().await?;
        let url = format!("{}/{}", self.url, name);
        Ok(Box::new(HttpDBFile {
            url,
            schema,
            client,
        }))
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let client = Client::new();
        let mut url = self.url.clone();
        url.push_str("/delete/");
        url.push_str(name);
        client.delete(url).send().await?;
        Ok(())
    }
}

impl HttpDBFile {
    async fn get_item(&self, url: &str) -> Result<Option<DbItem>> {
        let res = self.client.get(url).send().await?;
        if res.status().is_success() {
            Ok(Some(res.json().await?))
        } else if res.status() == StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            Err(anyhow::anyhow!("Failed to get item: {}", res.status()))
        }
    }
}

#[async_trait]
impl DBFile for HttpDBFile {
    fn schema(&self) -> DbSchema {
        self.schema.clone()
    }

    async fn list_items(&self) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items", self.url))
            .send()
            .await?
            .json()
            .await?;
        Ok(res)
    }

    async fn list_done(&self) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items/done", self.url))
            .send()
            .await?
            .json()
            .await?;
        Ok(res)
    }

    async fn list_undone(&self) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items/undone", self.url))
            .send()
            .await?
            .json()
            .await?;
        Ok(res)
    }

    async fn insert(&mut self, fields: &[DbField]) -> Result<()> {
        self.client
            .post(format!("{}/items", self.url))
            .json(fields)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn delete(&mut self, id: u32) -> Result<()> {
        self.client
            .delete(format!("{}/items/{}", self.url, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn get(&mut self, id: u32) -> Result<Option<DbItem>> {
        self.get_item(&format!("{}/items/{}", self.url, id)).await
    }

    async fn get_random(&mut self) -> Result<Option<DbItem>> {
        self.get_item(&format!("{}/items/random", self.url)).await
    }

    async fn done(&mut self, id: u32, time: chrono::NaiveDateTime) -> Result<()> {
        self.client
            .post(format!("{}/items/{}/done", self.url, id))
            .json(&time)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn undone(&mut self, id: u32) -> Result<()> {
        self.client
            .post(format!("{}/items/{}/undone", self.url, id))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn find(&self, item_name: &str) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items/search", self.url))
            .query(&[("q", item_name)])
            .send()
            .await?
            .json()
            .await?;
        Ok(res)
    }
}

use anyhow::Result;
use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode};

use crate::db::{DB, DBFile, DbError, DbField, DbItem, DbSchema};

pub struct HttpDB {
    url: String,
}

struct HttpDBFile {
    name: String,
    url: String,
    schema: DbSchema,
    client: Client,
}

impl HttpDB {
    pub fn new(url: String) -> Self {
        HttpDB { url }
    }
}

/// Converts error statuses into errors, mapping the ones with a `DbError` counterpart
async fn check_status(res: Response, name: &str) -> Result<Response> {
    match res.status() {
        StatusCode::NOT_FOUND => Err(DbError::NotFound(name.to_string()).into()),
        StatusCode::CONFLICT => Err(DbError::DbAlreadyExists(name.to_string()).into()),
        s if s.is_client_error() || s.is_server_error() => Err(server_error(res).await),
        _ => Ok(res),
    }
}

/// Builds an error from the message the server put in the response body
async fn server_error(res: Response) -> anyhow::Error {
    let status = res.status();
    match res.text().await {
        Ok(msg) if !msg.is_empty() => anyhow::anyhow!("Server error ({status}): {msg}"),
        _ => anyhow::anyhow!("Server error ({status})"),
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
        url.push_str("/file/");
        url.push_str(name);
        let schema = check_status(reqwest::get(url).await?, name)
            .await?
            .json()
            .await?;
        let url = format!("{}/{}", self.url, name);
        Ok(Box::new(HttpDBFile {
            name: name.to_string(),
            url,
            schema,
            client: Client::new(),
        }))
    }

    async fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>> {
        let mut url = self.url.clone();
        url.push_str("/file/");
        url.push_str(name);
        let client = Client::new();
        check_status(client.post(url).json(&schema).send().await?, name).await?;
        let url = format!("{}/{}", self.url, name);
        Ok(Box::new(HttpDBFile {
            name: name.to_string(),
            url,
            schema,
            client,
        }))
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let client = Client::new();
        let mut url = self.url.clone();
        url.push_str("/file/");
        url.push_str(name);
        check_status(client.delete(url).send().await?, name).await?;
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
            Err(server_error(res).await)
        }
    }
}

#[async_trait]
impl DBFile for HttpDBFile {
    fn schema(&self) -> DbSchema {
        self.schema.clone()
    }

    async fn close(&self) {}

    async fn list_items(&self) -> Result<Vec<DbItem>> {
        let res = self
            .client
            .get(format!("{}/items", self.url))
            .send()
            .await?;
        Ok(check_status(res, &self.name).await?.json().await?)
    }

    async fn list_done(&self) -> Result<Vec<DbItem>> {
        let res = self
            .client
            .get(format!("{}/items/done", self.url))
            .send()
            .await?;
        Ok(check_status(res, &self.name).await?.json().await?)
    }

    async fn list_undone(&self) -> Result<Vec<DbItem>> {
        let res = self
            .client
            .get(format!("{}/items/undone", self.url))
            .send()
            .await?;
        Ok(check_status(res, &self.name).await?.json().await?)
    }

    async fn insert(&mut self, fields: &[DbField]) -> Result<()> {
        let res = self
            .client
            .post(format!("{}/items", self.url))
            .json(fields)
            .send()
            .await?;
        check_status(res, &self.name).await?;
        Ok(())
    }

    async fn delete(&mut self, id: u32) -> Result<()> {
        let res = self
            .client
            .delete(format!("{}/items/{}", self.url, id))
            .send()
            .await?;
        check_status(res, &self.name).await?;
        Ok(())
    }

    async fn get(&mut self, id: u32) -> Result<Option<DbItem>> {
        self.get_item(&format!("{}/items/{}", self.url, id)).await
    }

    async fn get_random(&mut self) -> Result<Option<DbItem>> {
        self.get_item(&format!("{}/items/random", self.url)).await
    }

    async fn done(&mut self, id: u32, time: chrono::NaiveDateTime) -> Result<()> {
        let res = self
            .client
            .post(format!("{}/items/{}/done", self.url, id))
            .json(&time)
            .send()
            .await?;
        check_status(res, &self.name).await?;
        Ok(())
    }

    async fn undone(&mut self, id: u32) -> Result<()> {
        let res = self
            .client
            .post(format!("{}/items/{}/undone", self.url, id))
            .send()
            .await?;
        check_status(res, &self.name).await?;
        Ok(())
    }

    async fn find(&self, item_name: &str) -> Result<Vec<DbItem>> {
        let res = self
            .client
            .get(format!("{}/items/search", self.url))
            .query(&[("q", item_name)])
            .send()
            .await?;
        Ok(check_status(res, &self.name).await?.json().await?)
    }
}

use anyhow::Result;
use reqwest::{StatusCode, blocking::Client};

use crate::db::{DB, DBFile, DbField, DbItem, DbSchema};

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
        HttpDB { url: url }
    }
}

impl DB for HttpDB {
    fn list_files(&self) -> Result<Vec<String>> {
        let mut url = self.url.clone();
        url.push_str("/list");
        let res: Vec<String> = reqwest::blocking::get(url)?.json()?;
        Ok(res)
    }

    fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        let mut url = self.url.clone();
        url.push_str("/open/");
        url.push_str(name);
        let schema = reqwest::blocking::get(url)?.json()?;
        let url = format!("{}/{}", self.url, name);
        Ok(Box::new(HttpDBFile {
            url,
            schema,
            client: Client::new(),
        }))
    }

    fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>> {
        let mut url = self.url.clone();
        url.push_str("/create/");
        url.push_str(name);
        let client = Client::new();
        client.put(url).json(&schema).send()?;
        let url = format!("{}/{}", self.url, name);
        Ok(Box::new(HttpDBFile {
            url,
            schema,
            client,
        }))
    }

    fn delete(&self, name: &str) -> Result<()> {
        let client = Client::new();
        let mut url = self.url.clone();
        url.push_str("/delete/");
        url.push_str(name);
        client.delete(url).send()?;
        Ok(())
    }
}

impl HttpDBFile {
    fn get_item(&self, url: &str) -> Result<Option<DbItem>> {
        let res = self.client.get(url).send()?;
        if res.status().is_success() {
            Ok(Some(res.json()?))
        } else if res.status() == StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            Err(anyhow::anyhow!("Failed to get item: {}", res.status()))
        }
    }
}

impl DBFile for HttpDBFile {
    fn schema(&self) -> DbSchema {
        self.schema.clone()
    }

    fn list_items(&self) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items", self.url))
            .send()?
            .json()?;
        Ok(res)
    }

    fn list_done(&self) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items/done", self.url))
            .send()?
            .json()?;
        Ok(res)
    }

    fn list_undone(&self) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items/undone", self.url))
            .send()?
            .json()?;
        Ok(res)
    }

    fn insert(&self, fields: &[DbField]) -> Result<()> {
        self.client
            .post(format!("{}/items", self.url))
            .json(fields)
            .send()?;
        Ok(())
    }

    fn delete(&self, id: u32) -> Result<()> {
        self.client
            .delete(format!("{}/items/{}", self.url, id))
            .send()?;
        Ok(())
    }

    fn get(&self, id: u32) -> Result<Option<DbItem>> {
        self.get_item(&format!("{}/items/{}", self.url, id))
    }

    fn get_random(&self) -> Result<Option<DbItem>> {
        self.get_item(&format!("{}/items/random", self.url))
    }

    fn done(&self, id: u32, time: chrono::NaiveDateTime) -> Result<()> {
        self.client
            .post(format!("{}/items/{}/done", self.url, id))
            .json(&time)
            .send()?;
        Ok(())
    }

    fn undone(&self, id: u32) -> Result<()> {
        self.client
            .post(format!("{}/items/{}/undone", self.url, id))
            .send()?;
        Ok(())
    }

    fn find(&self, item_name: &str) -> Result<Vec<DbItem>> {
        let res: Vec<DbItem> = self
            .client
            .get(format!("{}/items/search", self.url))
            .query(&[("text", item_name)])
            .send()?
            .json()?;
        Ok(res)
    }
}

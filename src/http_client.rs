use anyhow::Result;

use crate::db::DB;

pub struct HttpDB {
    url: String,
}

impl HttpDB {
    pub fn new(url: String) -> Self {
        HttpDB { url: url, }
    }
}

impl DB for HttpDB {
    fn list_files(&self) -> Result<Vec<String>> {
        let mut url = self.url.clone();
        url.push_str("/list");
        let res: Vec<String> = reqwest::blocking::get(url)?.json()?;
        Ok(res)
    }
}

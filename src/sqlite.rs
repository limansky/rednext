use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    result,
};

use anyhow::{Context, Result, anyhow};
use rusqlite::{
    Connection, OptionalExtension, Params, Row, params,
    types::{FromSql, FromSqlError, FromSqlResult, Value, ValueRef},
};

use crate::db::{DB, DBFile, DbField, DbFieldDesc, DbFieldType, DbItem, DbSchema, DbValue};

pub struct SqliteDB {
    path: PathBuf,
}

impl SqliteDB {
    pub fn new(db_path: &Path) -> Self {
        SqliteDB {
            path: db_path.to_path_buf(),
        }
    }

    fn read_schema(conn: &Connection) -> rusqlite::Result<DbSchema> {
        let mut stmt = conn.prepare("SELECT name, datatype FROM schema ORDER BY idx")?;
        let fields = stmt
            .query_map([], |row| {
                let field_name: String = row.get(0)?;
                let field_type: DbFieldType = row.get(1)?;
                Ok(DbFieldDesc {
                    name: field_name,
                    field_type,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(DbSchema { fields })
    }

    fn write_schema(conn: &mut Connection, schema: &DbSchema) -> rusqlite::Result<()> {
        conn.execute(
            "CREATE TABLE schema (
            name TEXT PRIMARY KEY,
            datatype TEXT NOT NULL,
            idx INTEGER NOT NULL
          )",
            [],
        )?;
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO schema (name, datatype, idx) VALUES (?1, ?2, ?3)")?;
            for (idx, field) in schema.fields.iter().enumerate() {
                stmt.execute(params![
                    field.name,
                    field.field_type.to_string(),
                    idx as u32
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn create_items_table(conn: &mut Connection, schema: &DbSchema) -> rusqlite::Result<()> {
        let field_defs = schema
            .fields
            .iter()
            .map(|f| {
                let sql_type = match f.field_type {
                    DbFieldType::Text => "TEXT",
                    DbFieldType::Number => "NUMBER",
                    DbFieldType::Boolean => "BOOLEAN",
                    DbFieldType::DateTime => "TIMESTAMP",
                };
                format!("\"{}\" {}", f.name.as_str(), sql_type)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let create_table_sql = format!(
            "CREATE TABLE items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            {field_defs},
            done_at TIMESTAMP
          )"
        );
        conn.execute(create_table_sql.as_str(), [])?;
        Ok(())
    }
}

struct SqliteFile {
    connection: Connection,
    schema: DbSchema,
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
                    .context("Cannot read databases")
            } else {
                Err(anyhow!("Invalid DB path {:?}", self.path))
            }
        } else {
            Ok(vec![])
        }
    }

    fn create(&self, name: &str, schema: DbSchema) -> Result<Box<dyn DBFile>> {
        if !self.path.exists() {
            fs::create_dir_all(&self.path).context("Cannot create directory")?;
        }

        if !self.path.is_dir() {
            return Err(anyhow!("{:?} is not a directory", self.path));
        }

        let file_path = self.path.join(format!("{}.db", name));

        if file_path.exists() {
            return Err(anyhow!("Database file {:?} already exists", file_path));
        }

        let mut conn = Connection::open(&file_path).context("Cannot create DB file")?;
        Self::write_schema(&mut conn, &schema).context("Cannot write schema")?;
        Self::create_items_table(&mut conn, &schema).context("Cannot create items table")?;
        Ok(Box::new(SqliteFile {
            connection: conn,
            schema,
        }))
    }

    fn open(&self, name: &str) -> Result<Box<dyn DBFile>> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        let conn = Connection::open(path).context("Cannot open DB")?;
        let schema = Self::read_schema(&conn).context("Cannot read schema")?;

        Ok(Box::new(SqliteFile {
            connection: conn,
            schema,
        }))
    }

    fn delete(&self, name: &str) -> Result<()> {
        let mut path = self.path.clone();
        path.push(name);
        path.set_extension("db");
        fs::remove_file(path).context("Cannot delete file")
    }
}

impl SqliteFile {
    fn to_db_item(&self, row: &Row) -> rusqlite::Result<DbItem> {
        let fields: rusqlite::Result<Vec<DbField>> = self
            .schema
            .fields
            .iter()
            .map(|f| {
                let value = match f.field_type {
                    DbFieldType::Text => DbValue::Text(row.get(f.name.as_str())?),
                    DbFieldType::Number => DbValue::Number(row.get(f.name.as_str())?),
                    DbFieldType::Boolean => DbValue::Boolean(row.get(f.name.as_str())?),
                    DbFieldType::DateTime => DbValue::DateTime(row.get(f.name.as_str())?),
                };
                Ok(DbField {
                    name: f.name.clone(),
                    value,
                })
            })
            .collect();
        Ok(DbItem {
            id: row.get("id")?,
            fields: fields?,
            completed_at: row.get("done_at")?,
        })
    }

    fn select_items<P: Params>(
        &self,
        filter: Option<&str>,
        params: P,
        order_by: Option<&str>,
    ) -> Result<Vec<DbItem>> {
        let ord = order_by.unwrap_or("id");
        let base_query = self.base_select();
        let mut q = filter
            .map(|c| format!("{base_query} WHERE {c}"))
            .unwrap_or(base_query);
        q.push_str(&format!(" ORDER BY {ord}"));
        let mut stmt = self.connection.prepare(&q)?;
        let iter = stmt.query_map(params, |row| self.to_db_item(row))?;
        iter.collect::<rusqlite::Result<Vec<_>>>()
            .context("Item query error")
    }

    fn base_select(&self) -> String {
        let fields = self
            .schema
            .fields
            .iter()
            .map(|f| format!("\"{}\"", f.name.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        format!("SELECT id, {fields}, done_at FROM items")
    }
}

impl DBFile for SqliteFile {
    fn schema(&self) -> DbSchema {
        self.schema.clone()
    }

    fn insert(&self, fields: &[DbField]) -> Result<()> {
        let field_names = fields
            .iter()
            .map(|f| format!("\"{}\"", f.name.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=fields.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let values: Vec<Value> = fields
            .iter()
            .map(|f| match &f.value {
                DbValue::Text(s) => s.clone().into(),
                DbValue::Number(n) => (*n).into(),
                DbValue::Boolean(b) => (*b).into(),
                DbValue::DateTime(dt) => dt.format("%Y-%m-%dT%H:%M:%S").to_string().into(),
            })
            .collect::<Vec<_>>();
        self.connection
            .execute(
                format!("INSERT INTO items ({field_names}) VALUES({placeholders})").as_str(),
                rusqlite::params_from_iter(values),
            )
            .context("Cannot insert item")?;
        Ok(())
    }

    fn delete(&self, id: u32) -> Result<()> {
        self.connection
            .execute("DELETE FROM items WHERE id=?1", params![id])
            .context("Cannot delete item")?;
        Ok(())
    }

    fn list_items(&self) -> Result<Vec<DbItem>> {
        self.select_items(None, [], None)
    }

    fn list_done(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NOT NULL"), [], Some("done_at"))
    }

    fn list_undone(&self) -> Result<Vec<DbItem>> {
        self.select_items(Some("done_at IS NULL"), [], None)
    }

    fn find(&self, item_name: &str) -> Result<Vec<DbItem>> {
        let pattern = format!("%{item_name}%");
        let fields = self.schema.fields.iter().flat_map(|f| {
            if f.field_type == DbFieldType::Text {
                Some(format!("{} LIKE ?1", f.name))
            } else {
                None
            }
        });
        let filter = fields.collect::<Vec<_>>().join(" OR ");
        self.select_items(Some(filter.as_str()), params![pattern], None)
    }

    fn get_random(&self) -> Result<Option<DbItem>> {
        let base_query = self.base_select();
        self.connection
            .query_one(
                format!(
                    "{base_query}
                     WHERE done_at IS NULL
                     ORDER BY random()
                     LIMIT 1"
                )
                .as_str(),
                [],
                |row| self.to_db_item(row),
            )
            .optional()
            .context("Query error")
    }

    fn get(&self, id: u32) -> Result<Option<DbItem>> {
        let base_query = self.base_select();
        self.connection
            .query_one(
                format!("{base_query} WHERE id=?1").as_str(),
                params![id],
                |row| self.to_db_item(row),
            )
            .optional()
            .context("Query error")
    }

    fn done(&self, id: u32, time: chrono::NaiveDateTime) -> Result<()> {
        let count = self
            .connection
            .execute(
                "UPDATE items SET done_at=?1 WHERE id =?2",
                params![time, id],
            )
            .context("Cannot update item ")?;
        if count == 1 {
            Ok(())
        } else {
            Err(anyhow!("Expect exact one item, but got {}", count))
        }
    }

    fn undone(&self, id: u32) -> Result<()> {
        let count = self
            .connection
            .execute("UPDATE items SET done_at=NULL WHERE id =?1", params![id])
            .context("Cannot update item")?;

        if count == 1 {
            Ok(())
        } else {
            Err(anyhow!("Item with id {id} is not found"))
        }
    }
}

impl FromSql for DbFieldType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|e| FromSqlError::Other(Box::new(e)))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime};
    use rusqlite::Connection;

    use crate::{
        db::{DBFile, DbField, DbFieldDesc, DbFieldType, DbSchema, DbValue},
        sqlite::SqliteFile,
    };

    fn create_file() -> SqliteFile {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE items(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            txt TEXT,
            due TIMESTAMP,
            n NUMBER,
            bool BOOLEAN,
            done_at TIMESTAMP,
            comment TEXT
          )",
            [],
        )
        .unwrap();
        let schema = DbSchema {
            fields: vec![
                DbFieldDesc::new("txt", DbFieldType::Text),
                DbFieldDesc::new("due", DbFieldType::DateTime),
                DbFieldDesc::new("bool", DbFieldType::Boolean),
                DbFieldDesc::new("n", DbFieldType::Number),
            ],
        };
        SqliteFile {
            connection: conn,
            schema: schema,
        }
    }

    #[test]
    fn test_insert() {
        let file = create_file();
        assert!(file.list_items().unwrap().is_empty());
        file.insert(&[
            DbField {
                name: "txt".to_string(),
                value: DbValue::Text("task 1".to_string()),
            },
            DbField {
                name: "due".to_string(),
                value: DbValue::DateTime(
                    chrono::NaiveDate::from_ymd_opt(2024, 7, 1)
                        .unwrap()
                        .and_hms_opt(1, 20, 0)
                        .unwrap(),
                ),
            },
            DbField {
                name: "bool".to_string(),
                value: DbValue::Boolean(true),
            },
            DbField {
                name: "n".to_string(),
                value: DbValue::Number(42),
            },
        ])
        .unwrap();

        let items = file.list_items().unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.id, 1);
        assert_eq!(item.fields.len(), 4);
        assert_eq!(item.fields[0].name, "txt");
        assert_eq!(item.fields[0].value, DbValue::Text("task 1".to_string()));
        assert_eq!(item.fields[1].name, "due");
        assert_eq!(
            item.fields[1].value,
            DbValue::DateTime(
                NaiveDate::from_ymd_opt(2024, 7, 1)
                    .unwrap()
                    .and_hms_opt(1, 20, 0)
                    .unwrap(),
            )
        );
        assert_eq!(item.fields[2].name, "bool");
        assert_eq!(item.fields[2].value, DbValue::Boolean(true));
        assert_eq!(item.fields[3].name, "n");
        assert_eq!(item.fields[3].value, DbValue::Number(42));
    }
}

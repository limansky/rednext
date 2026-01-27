use std::{
    error::Error,
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;
use chrono::{Local, NaiveDate, NaiveDateTime};
use clap::{Args, Parser, Subcommand, ValueEnum};
use comfy_table::Table;
use console::Style;
use csv::{ReaderBuilder, Trim};
use dialoguer::{Confirm, Input, Select};
use dirs::config_dir;

use crate::{
    db::{DB, DBFile, DbField, DbFieldDesc, DbFieldType, DbItem, DbSchema, DbValue},
    sqlite::SqliteDB,
};

mod db;
mod sqlite;

#[derive(Subcommand, Debug)]
enum Action {
    /// List available files
    List,

    /// Operations within items
    Items(ItemsParams),

    /// Create new file
    #[command(arg_required_else_help = true)]
    New {
        /// New file name
        name: String,
        /// Import data from CSV file
        from_file: Option<PathBuf>,
        /// Use custom CSV delimiter
        #[clap(long, short='d', value_parser=delimiter_parser)]
        delimiter: Option<u8>,

        /// Specify if CSV file has no header
        #[clap(long, action)]
        no_header: bool,
    },

    /// Delete file
    #[command(arg_required_else_help = true)]
    Delete { name: String },
}

#[derive(Debug, Args)]
struct ItemsParams {
    /// File name
    name: String,
    #[command(subcommand)]
    action: ItemsAction,
}

#[derive(Subcommand, Debug)]
enum ItemsAction {
    /// List items
    List {
        #[arg(
            default_value_t = ListWhat::All,
            value_enum
        )]
        what: ListWhat,
    },

    /// Interactively add a new item
    Add,

    /// Delete item by ID
    Delete { id: u32 },

    /// Import CSV file
    Import {
        /// file to be imported
        file_name: PathBuf,

        /// Use custom CSV delimiter
        #[clap(long, short='d', value_parser=delimiter_parser)]
        delimiter: Option<u8>,

        /// Specify if CSV file has no header
        #[clap(long, action)]
        no_header: bool,
    },

    /// Find item by name
    Find { name: String },

    /// Get item by ID
    Get { id: u32 },

    /// Get random item
    GetRandom,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum ListWhat {
    All,
    Done,
    Undone,
}

impl std::fmt::Display for ListWhat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_possible_value()
            .expect("no values are skipped")
            .get_name()
            .fmt(f)
    }
}

#[derive(Parser, Debug)]
#[command(about = "Simple random tasks manager")]
struct Params {
    #[command(subcommand)]
    action: Action,
}

fn delimiter_parser(s: &str) -> Result<u8, String> {
    if s.starts_with("0x")
        && let Ok(num) = u8::from_str_radix(&s[2..], 16)
    {
        Ok(num)
    } else {
        let sv = s.chars().collect::<Vec<_>>();
        if sv.len() == 1 {
            if sv[0].is_ascii() {
                Ok(sv[0] as u8)
            } else {
                Err("Delimiter must be an ASCII character".to_string())
            }
        } else {
            Err("Delimiter must be a single character or a HEX number".to_string())
        }
    }
}

fn main() {
    let params = Params::parse();
    let mut db_path = config_dir().unwrap();
    db_path.push("rednext");
    let db = SqliteDB::new(&db_path);
    match params.action {
        Action::List => list(&db),
        Action::Items(ip) => {
            let file = db.open(&ip.name).unwrap();
            match ip.action {
                ItemsAction::List { what } => list_items(file.as_ref(), what),
                ItemsAction::Add => add_item(file.as_ref()),
                ItemsAction::Delete { id } => delete_item(file.as_ref(), id),
                ItemsAction::Import {
                    file_name,
                    delimiter,
                    no_header,
                } => import_csv(
                    file.as_ref(),
                    &file_name,
                    no_header,
                    delimiter,
                )
                .unwrap(),
                ItemsAction::Get { id } => get(file.as_ref(), id),
                ItemsAction::GetRandom => get_random(file.as_ref()),
                ItemsAction::Find { name } => find_by_name(file.as_ref(), &name),
            }
        }
        Action::New {
            name,
            from_file,
            delimiter,
            no_header,
        } => new_file(&db, &name, from_file, delimiter, no_header).unwrap(),
        Action::Delete { name } => delete(&db, &name),
    }
}

fn list(db: &impl DB) {
    let files = db.list_files().unwrap();
    for (i, name) in (1..).zip(files.into_iter()) {
        println!("{}. {}", i, name);
    }
}

fn list_items(file: &dyn DBFile, what: ListWhat) {
    let items = match what {
        ListWhat::All => file.list_items(),
        ListWhat::Done => file.list_done(),
        ListWhat::Undone => file.list_undone(),
    }
    .unwrap();

    let stat_style = Style::new().bold();
    let mut done_count = 0;
    let total = items.len();
    let mut table = Table::new();
    let mut header = vec!["ID".to_string()];
    file.schema().fields.into_iter().for_each(|f| {
        header.push(f.name);
    });
    header.push("Done".to_string());
    table.load_preset("││──╞═╪╡│    ┬┴┌┐└┘").set_header(header);
    for i in items {
        let done_str = i
            .completed_at
            .map_or("".to_string(), |dt| dt.format("%Y-%m-%d %H:%M").to_string());
        let mut row = Vec::with_capacity(i.fields.len() + 2);
        row.push(i.id.to_string());
        row.extend(i.fields.iter().map(|f| f.value.to_string()));
        row.push(done_str);
        if i.completed_at.is_some() {
            done_count += 1;
        }
        table.add_row(row);
    }
    println!("{table}");
    if what == ListWhat::All {
        let stat = format!(
            "Total: done {} of {} ({:.2}%)",
            done_count,
            total,
            (done_count as f64) / (total as f64) * 100.0
        );
        println!("{}", stat_style.apply_to(stat));
    }
}

#[derive(Debug)]
struct DateParseError;

impl Error for DateParseError {}

#[derive(Clone)]
struct Date(NaiveDateTime);

impl FromStr for Date {
    type Err = DateParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let dt_fmt = "%Y-%m-%d %H:%M:%S";
        let d_fmt = "%Y-%m-%d";
        NaiveDateTime::parse_from_str(s, dt_fmt)
            .or(NaiveDate::parse_from_str(s, d_fmt).map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
            .map(Date)
            .map_err(|_| DateParseError)
    }
}

impl Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for DateParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to parse date")
    }
}

fn add_item(file: &dyn DBFile) {
    let mut fields = Vec::new();
    for field in file.schema().fields.iter() {
        let val = match field.field_type {
            DbFieldType::Text => {
                let input: String = Input::new()
                    .with_prompt(&field.name)
                    .interact_text()
                    .unwrap();
                DbValue::Text(input)
            }
            DbFieldType::Number => {
                let input: i32 = Input::new()
                    .with_prompt(&field.name)
                    .interact_text()
                    .unwrap();
                DbValue::Number(input)
            }
            DbFieldType::Boolean => {
                let input: bool = Confirm::new().with_prompt(&field.name).interact().unwrap();
                DbValue::Boolean(input)
            }
            DbFieldType::DateTime => {
                let input: Date = Input::new()
                    .with_prompt(&field.name)
                    .interact_text()
                    .unwrap();
                DbValue::DateTime(input.0)
            }
        };
        fields.push(DbField {
            name: field.name.clone(),
            value: val,
        });
    }
    file.insert(&fields).unwrap();
}

fn delete_item(file: &dyn DBFile, id: u32) {
    file.delete(id).unwrap();
}

fn get(file: &dyn DBFile, id: u32) {
    if let Some(item) = file.get(id).unwrap() {
        if item.completed_at.is_some() {
            let conf = Confirm::new()
                .with_prompt("Already done. Mark as undone?")
                .interact()
                .unwrap();
            if conf {
                file.undone(id).unwrap();
            }
        } else {
            mark_done(file, item);
        }
    } else {
        println!("Item with id {id} doesn't exist");
    }
}

fn item_fields_to_string(item: &DbItem) -> String {
    item.fields
        .iter()
        .map(|f| f.value.to_string())
        .collect::<Vec<_>>()
        .join(" - ")
}

fn get_random(file: &dyn DBFile) {
    if let Some(item) = file.get_random().unwrap() {
        let fields_str = item_fields_to_string(&item);
        println!("Random item is {}: {}", item.id, fields_str);
        mark_done(file, item);
    } else {
        println!("All items are complete");
    }
}

fn mark_done(file: &dyn DBFile, item: DbItem) {
    let done = Confirm::new()
        .with_prompt("Mark as done?")
        .default(true)
        .interact()
        .unwrap();
    if done {
        file.done(item.id, Local::now().naive_local()).unwrap();
    }
}

fn find_by_name(file: &dyn DBFile, name: &str) {
    let items = file.find(name).unwrap();
    if !items.is_empty() {
        for item in items {
            let fields_str = item_fields_to_string(&item);
            println!("{}. {}", item.id, fields_str);
        }
    } else {
        println!("No matching items found");
    }
}

fn enter_schema() -> DbSchema {
    let mut fields = Vec::new();
    loop {
        let field_name: String = Input::new()
            .with_prompt("Enter field name (or leave empty to finish)")
            .allow_empty(true)
            .interact_text()
            .unwrap();
        if field_name.is_empty() {
            break;
        }
        let field_type = Select::new()
            .with_prompt("Choose field type")
            .items(["Text", "Number", "Boolean", "Timestamp"])
            .default(0)
            .interact()
            .unwrap();
        fields.push(DbFieldDesc::new(
            &field_name,
            match field_type {
                0 => DbFieldType::Text,
                1 => DbFieldType::Number,
                2 => DbFieldType::Boolean,
                3 => DbFieldType::DateTime,
                _ => unreachable!(),
            },
        ));
    }
    DbSchema { fields }
}

fn new_file(
    db: &impl DB,
    name: &str,
    source: Option<PathBuf>,
    delimiter: Option<u8>,
    no_header: bool,
) -> anyhow::Result<()> {
    let schema = enter_schema();
    let file = db
        .create(name, schema)
        .context("Failed to create a new file")?;

    if let Some(source) = source {
        import_csv(file.as_ref(), &source, no_header, delimiter)
    } else {
        Ok(())
    }
}

fn import_csv(
    file: &dyn DBFile,
    file_name: &Path,
    no_header: bool,
    delimiter: Option<u8>,
) -> anyhow::Result<()> {
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter.unwrap_or(b','))
        .has_headers(!no_header)
        .trim(Trim::All)
        .from_path(file_name)
        .unwrap();
    for r in reader.records() {
        let record = r.context("Incorrect CSV record")?;
        let mut fields = Vec::new();
        for (i, field_desc) in file.schema().fields.iter().enumerate() {
            let str = record.get(i).context("Not enough fields in CSV record")?;
            let value = match field_desc.field_type {
                DbFieldType::Text => DbValue::Text(str.to_string()),
                DbFieldType::Number => {
                    let num: i32 = str
                        .parse()
                        .context(format!("Failed to parse number '{str}'"))?;
                    DbValue::Number(num)
                }
                DbFieldType::Boolean => {
                    let b: bool = str.parse().context("Failed to parse boolean")?;
                    DbValue::Boolean(b)
                }
                DbFieldType::DateTime => {
                    let date: Date = str.parse().context("Failed to parse date")?;
                    DbValue::DateTime(date.0)
                }
            };
            fields.push(DbField {
                name: field_desc.name.clone(),
                value,
            });
        }
        file.insert(&fields).unwrap();
    }

    Ok(())
}

fn delete(db: &impl DB, name: &str) {
    let confirmation = Confirm::new()
        .with_prompt(format!("Are you sure you want to delete file {name}?"))
        .interact()
        .unwrap();

    if confirmation {
        db.delete(name).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    use crate::Date;

    #[test]
    fn test_parse_date() {
        let date: Date = "2026-01-11".parse().unwrap();
        assert_eq!(
            date.0,
            NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 1, 11).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            )
        );
    }

    #[test]
    fn test_parse_date_time() {
        let date: Date = "2026-01-11 15:30:00".parse().unwrap();
        assert_eq!(
            date.0,
            NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 1, 11).unwrap(),
                NaiveTime::from_hms_opt(15, 30, 0).unwrap(),
            )
        );
    }
}

use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use chrono::Local;
use clap::{Args, Parser, Subcommand, ValueEnum};
use console::Style;
use dialoguer::Confirm;
use dirs::config_dir;

use crate::{
    db::{DB, DBFile, DbItem},
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
        name: String,
        from_file: Option<String>,
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

    /// Add new Item
    Add { name: String },

    /// Delete item by ID
    Delete { id: u64 },

    /// Find item by name
    Find { name: String },

    /// Get item by ID
    Get { id: u64 },

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
                ItemsAction::Add { name } => add_item(file.as_ref(), &name),
                ItemsAction::Delete { id } => delete_item(file.as_ref(), id),
                ItemsAction::Get { id } => get(file.as_ref(), id),
                ItemsAction::GetRandom => get_random(file.as_ref()),
                ItemsAction::Find { name } => find_by_name(file.as_ref(), &name),
            }
        }
        Action::New { name, from_file } => new_file(&db, &name, from_file),
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
    for i in items {
        let line = format!("{}. {}", i.id, i.name);
        if i.completed_at.is_none() {
            println!("{line}");
        } else {
            done_count += 1;
            println!("{} {}", line, "\u{2705}");
        }
    }
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

fn add_item(file: &dyn DBFile, item_name: &str) {
    file.insert(item_name).unwrap();
}

fn delete_item(file: &dyn DBFile, id: u64) {
    file.delete(id).unwrap();
}

fn get(file: &dyn DBFile, id: u64) {
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

fn get_random(file: &dyn DBFile) {
    if let Some(item) = file.get_random().unwrap() {
        println!("random Item is {}: {}", item.id, item.name);
        mark_done(file, item);
    } else {
        println!("All items are complete");
    }
}

fn mark_done(file: &dyn DBFile, item: DbItem) {
    let done = Confirm::new()
        .with_prompt("Mark as done?")
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
            println!("{}. {}", item.id, item.name);
        }
    } else {
        println!("No matching items found");
    }
}

fn new_file(db: &impl DB, name: &str, source: Option<String>) {
    let file = db.open(name).unwrap();
    if let Some(from_file) = source {
        let ff = File::open(from_file).unwrap();
        let lines = BufReader::new(ff).lines();
        for line in lines.map_while(Result::ok) {
            file.insert(&line).unwrap();
        }
    }
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

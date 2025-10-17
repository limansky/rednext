use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use chrono::Local;
use clap::{Args, Parser, Subcommand};
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
    List,

    /// Add new Item
    Add { name: String },

    /// Delete item by ID
    Delete { id: u64 },

    /// Find item by name
    Find,

    /// Get item by ID
    Get,

    /// Get random item
    GetRandom,
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
                ItemsAction::List => list_items(file.as_ref()),
                ItemsAction::Add { name } => add_item(file.as_ref(), &name),
                ItemsAction::GetRandom => get_random(file.as_ref()),
                _ => todo!(),
            }
        }
        // Action::ListItems { name } => list_items(&db, &name),
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

fn list_items(file: &dyn DBFile) {
    let items = file.list_items().unwrap();
    let done = Style::new().strikethrough();
    for i in items {
        let line = format!("{}. {}", i.id, i.name);
        if i.completed_at.is_none() {
            println!("{line}");
        } else {
            println!("{}", done.apply_to(line));
        }
    }
}

fn add_item(file: &dyn DBFile, item_name: &str) {
    file.insert(item_name).unwrap();
}

fn get_random(file: &dyn DBFile) {
    if let Some(item) = file.get_random().unwrap() {
        println!("random Item is {}: {}", item.id, item.name);
        process_item(file, item);
    } else {
        println!("All items are complete");
    }
}

fn process_item(file: &dyn DBFile, item: DbItem) {
    if item.completed_at.is_some() {
        println!("Already completed");
    } else {
        let done = Confirm::new()
            .with_prompt("Mark as done?")
            .interact()
            .unwrap();
        if done {
            file.done(item.id, Local::now().naive_local()).unwrap();
        }
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

use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use dirs::config_dir;

use crate::{db::DB, sqlite::SqliteDB};

mod db;
mod sqlite;

#[derive(Subcommand, Debug)]

enum Action {
    /// List available files
    List,

    /// List items in the file
    #[command(arg_required_else_help = true)]
    ListItems { name: String },

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

#[derive(Parser, Debug)]
#[command(about = "Simple random tasks manager")]
struct Args {
    #[command(subcommand)]
    action: Action,
}

fn main() {
    let args = Args::parse();
    let mut db_path = config_dir().unwrap();
    db_path.push("rednext");
    let db = SqliteDB::new(&db_path);
    match args.action {
        Action::List => list(&db),
        Action::ListItems { name } => list_items(&db, &name),
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

fn list_items(db: &impl DB, name: &str) {
    let file = db.open(&name).unwrap();

    let items = file.list_items().unwrap();
    for i in items {
        println!("{}. {}", i.id, i.name);
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

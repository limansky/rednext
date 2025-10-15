use clap::{Parser, Subcommand};
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
    New { name: String },

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
        _ => unimplemented!("Not implemented yet"),
    }
}

fn list(db: &impl DB) {

    let files = db.list_files().unwrap();

    for (i, name) in (1..).zip(files.into_iter()) {
        println!("{}. {}", i, name);
    }
}

use std::{ffi::OsStr, fs};

use clap::{Parser, Subcommand};
use dirs::config_dir;

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
    match args.action {
        Action::List => list(),
        _ => unimplemented!("Not implemented yet"),
    }
}

fn list() {
    let mut db_path = config_dir().unwrap();
    db_path.push("rednext");

    if db_path.exists() {
        if db_path.is_dir() {
            println!("looking in {:?}", db_path);
            let files = fs::read_dir(db_path)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|d| d.path().extension() == Some(OsStr::new("db")))
                .flat_map(|d| {
                    d.path()
                        .file_stem()
                        .and_then(|x| x.to_str().map(|s| s.to_string()))
                });
            for (i, name) in (1..).zip(files.into_iter()) {
                println!("{}. {}", i, name);
            }
        } else {
            eprintln!("Invalid DB path {:?}", db_path);
        }
    }
}

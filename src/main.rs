use std::{ffi::OsStr, fs};

use clap::{Parser, ValueEnum};
use dirs::config_dir;

#[derive(ValueEnum, Debug, Clone)]
enum Action {
    List,
    ListItems,
    New,
    Delete,
}

#[derive(Parser, Debug)]
struct Args {
    action: Action,
}

fn main() {
    let args = Args::parse();
    list();
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

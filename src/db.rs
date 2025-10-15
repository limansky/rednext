use core::fmt;

pub trait DB {
    fn list_files(&self) -> Result<Vec<String>, Problem>;
}

#[derive(Debug)]
pub enum Problem {
    IOError(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::IOError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

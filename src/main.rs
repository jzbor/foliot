use chrono::offset::Local;
use chrono::{DateTime, DurationRound};
use clap::Parser;
use serde::{Serialize, Deserialize};
use std::fmt::Display;
use std::fs;
use std::path::*;

/// Tracks time for tasks
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// The namespace to apply the command to
    #[clap(short, long, default_value_t = String::from(DEFAULT_NAMESPACE))]
    namespace: String,
    // TODO handle empty values

    #[clap(subcommand)]
    command: Command,
}

/// Record of a started clock
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ClockinTimestamp {
    start_time: DateTime<Local>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, clap::Subcommand)]
enum Command {
    /// Abort current timer
    Abort,

    /// Start the timer
    Clockin,

    /// Stop the timer and add save the entry
    Clockout {
        /// Comment on the clock entry
        #[clap(short, long)]
        comment: Option<String>,
    },
}

/// Human readable duration (no more precise than a minute)
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct Duration {
    hours: i64,
    minutes: i64,
}

/// Clock entry
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct Entry {
    /// Time the clock was started
    start_time: DateTime<Local>,

    /// Time the clock ended
    end_time: DateTime<Local>,

    /// Total time elapsed
    duration: Duration,

    /// Optional comment
    comment: Option<String>,
}


const DEFAULT_NAMESPACE: &str = "default";
const XDG_DIR_PREFIX: &str = "pendulum";


impl Command {
    /// Execute a command with the given arguments
    fn execute(&self, args: &Args) -> Result<(), String> {
        match self {
            Self::Abort => abort(args),
            Self::Clockin => clockin(args),
            Self::Clockout { comment } => clockout(comment.clone(), args),
        }
    }
}

impl ClockinTimestamp {
    /// Creates a [ClockinTimestamp] referencing the date and time of the function call
    fn now() -> Self {
        return ClockinTimestamp { start_time: now() }
    }

    /// Relative path to the file that contains the last clockin timestamp
    fn relative_path(namespace: &str) -> PathBuf {
        PathBuf::from(format!("{}-clockin", namespace))
            .with_extension("yaml")
    }
}

impl Entry {
    /// Create a new clock entry
    fn create(start_time: DateTime<Local>, end_time: DateTime<Local>, comment: Option<String>) -> Self {
        return Entry {
            start_time, end_time, comment,
            duration: (end_time - start_time).into(),
        };
    }

    /// Relative path to the file that entries are collected in
    fn relative_path(namespace: &str) -> PathBuf {
        PathBuf::from(format!("{}", namespace))
            .with_extension("yaml")
    }
}

impl Display for Duration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}h", self.hours, self.minutes)
    }
}

impl Into<Duration> for chrono::Duration {
    fn into(self) -> Duration {
        Duration {
            hours: self.num_hours(),
            minutes: self.num_minutes() % 60
        }
    }
}


/// Abort the currently running clock by deleting its file
fn abort(args: &Args) -> Result<(), String> {
    let path = ClockinTimestamp::relative_path(&args.namespace);

    if !data_file_exists(&path).unwrap() {
        Err(format!("Clock is not running for namespace '{}'", args.namespace))
    } else {
        println!("Aborting clock for namespace '{}'", args.namespace);
        remove_data_file(&path)
    }
}

/// Start a new clock by creating a new clockin file
fn clockin(args: &Args) -> Result<(), String> {
    let path = ClockinTimestamp::relative_path(&args.namespace);
    let timestamp = ClockinTimestamp::now();

    if data_file_exists(&path).unwrap() {
        return Err(format!("Clock-in file '{}' already exists.\nPlease remove it before continuing.", path.to_string_lossy()));
    }

    println!("Starting clock for namespace {} ({})", args.namespace, timestamp.start_time);
    write_data_file(&path, timestamp)
}

/// Stop the clock and entry to the entries file
fn clockout(comment: Option<String>, args: &Args)  -> Result<(), String> {
    let path = Entry::relative_path(&args.namespace);
    let mut entries = if data_file_exists(&path).unwrap() {
        read_data_file(&path)?
    } else {
        Vec::new()
    };

    let clockin_path = ClockinTimestamp::relative_path(&args.namespace);
    let clockin_timestamp: ClockinTimestamp = read_data_file(&clockin_path)
        .map_err(|_| "No clockin file found".to_owned())?;

    let entry = Entry::create(clockin_timestamp.start_time, now(), comment);

    println!("Adding entry for namespace '{}':", args.namespace);
    println!("\t starting at {}", entry.start_time);
    println!("\t ending at   {}", entry.end_time);
    println!("\t duration:   {}", entry.duration);
    if let Some(comment) = &entry.comment {
        println!("\t comment:    {}", comment);
    }

    entries.push(entry);
    write_data_file(&path, entries)?;
    remove_data_file(&clockin_path)
}

/// Check whether a file with the relative path `path` exists in the data directory
fn data_file_exists(path: &impl AsRef<Path>) -> Result<bool, String> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;
    Ok(xdg_dirs.find_data_file(path).is_some())
}

/// Return current time in the current timezone
fn now() -> DateTime<Local> {
    Local::now().duration_round(chrono::Duration::minutes(1)).unwrap()
}

/// Deserialize a file with the relative path `path` in the data directory
fn read_data_file<T: for<'a> Deserialize<'a>>(path: &impl AsRef<Path>) -> Result<T, String> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;
    let abs_path = xdg_dirs.find_data_file(path)
        .ok_or(format!("Path not found"))?;
    let content = fs::read(abs_path)
        .map_err(|e| e.to_string())?;
    serde_yaml::from_slice(&content)
        .map_err(|e| e.to_string())
}

/// Delete a file with the relative path `path` in the data directory
fn remove_data_file(path: &impl AsRef<Path>) -> Result<(), String> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;
    let abs_path = xdg_dirs.find_data_file(path)
        .ok_or(format!("Path not found"))?;
    fs::remove_file(abs_path)
        .map_err(|e| e.to_string())
}

/// Serialize a file with the relative path `path` in the data directory
fn write_data_file(path: &impl AsRef<Path>, data: impl Serialize) -> Result<(), String> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;
    let abs_path = xdg_dirs.place_data_file(path)
        .map_err(|e| e.to_string())?;
    let content = serde_yaml::to_string(&data)
        .map_err(|e| e.to_string())?;
    fs::write(abs_path, content)
        .map_err(|e| e.to_string())
}


fn main() {
    let args = Args::parse();

    if args.namespace.is_empty() {
        println!("The namespace parameter must not be empty");
        std::process::exit(1);
    }

    let command = args.command.clone();
    if let Err(e) = command.execute(&args) {
        println!("Error: {}", e);
        std::process::exit(1);
    }
}

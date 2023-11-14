use chrono::offset::Local;
use chrono::{DateTime, DurationRound, NaiveDateTime, TimeZone, NaiveDate, NaiveTime, Months};
use clap::Parser;
use regex::Regex;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::env;
use std::fmt::Display;
use std::fs;
use std::iter;
use std::ops::Add;
use std::path::*;
use std::process;
use tabled::*;
use tabled::color::Color;
use tabled::object::*;

/// Tracks time for tasks
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// The namespace to apply the command to
    #[clap(short, long, default_value_t = String::from(DEFAULT_NAMESPACE))]
    namespace: String,
    // TODO handle empty values

    /// Run `git commit -am "[<namespace>] <action>"` afterwards
    #[clap(short, long)]
    git_commit: bool,

    /// Pull, rebase and push git repository afterwards
    #[clap(short('p'), long)]
    git_push: bool,

    #[clap(subcommand)]
    command: Command,
}

/// Record of a started clock
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ClockinTimestamp {
    start_time: DateTime<Local>,
}

#[derive(Clone, Debug, PartialEq, clap::Subcommand)]
enum Command {
    /// Abort current timer
    Abort {},

    /// Clock an arbitrary time
    Clock {
        /// Number of hours to log (floating point number)
        hours: f64,

        /// Starting time (format: %Y-%m-%dT%H:%M:%S, eg. 2015-09-18T23:56:04)
        #[clap(short, long, value_parser = parse_starting_value)]
        starting: Option<NaiveDateTime>,

        /// Comment on the clock entry
        comment: Option<String>,
    },

    /// Start the timer
    Clockin {
        /// Starting time (format: %Y-%m-%dT%H:%M:%S, eg. 2015-09-18T23:56:04)
        #[clap(short, long, value_parser = parse_starting_value)]
        starting: Option<NaiveDateTime>,
    },

    /// Stop the timer and add save the entry
    Clockout {
        /// Comment on the clock entry
        comment: Option<String>,
    },

    /// Edit entries or clockin file
    Edit {
        /// Edit clockin file
        #[clap(short, long)]
        clockin: bool,
    },

    /// Execute git command in foliot directory
    Git {
        /// Arguments to pass to git
        git_args: Vec<String>,
    },

    /// Print path to the data to output
    Path {
        /// Print path to the given namespace entry file
        #[clap(short, long)]
        namespace: Option<String>,
    },

    /// Show entries in a table
    Show {
        /// Filter entries with regex
        #[clap(short, long)]
        filter: Option<String>,

        /// Only show last n entries (0 to show all)
        #[clap(short, long, default_value_t = 30)]
        tail: usize,

        /// Wrap content column at x chars
        #[clap(short, long, default_value_t = 80)]
        wrap: usize,
    },

    /// Print current status of clock timer
    Status {},

    /// Create a per-month summary
    Summarize {
        /// Filter entries with regex
        #[clap(short, long)]
        filter: Option<String>,

        /// Only show last n entries (0 to show all)
        #[clap(short, long, default_value_t = 30)]
        tail: usize,
    },
}

/// Human readable duration (no more precise than a minute)
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct HumanDuration {
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

    /// Optional comment
    comment: Option<String>,
}

/// Entry formatted for displaying in human-readable form
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Tabled)]
struct TableEntry {
    date: NaiveDate,
    from: NaiveTime,
    to: NaiveTime,
    duration: HumanDuration,
    comment: String,
}

/// Entry formatted for displaying a summary for a month
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Tabled)]
struct SummaryTableItem {
    month: String,

    #[tabled(rename = "total hours")]
    total_hours: HumanDuration,

    #[tabled(rename = "hours / week")]
    hours_per_week: String,

    days: usize,

    #[tabled(rename = "entries")]
    nitems: usize,
}


const DEFAULT_NAMESPACE: &str = "default";
const XDG_DIR_PREFIX: &str = "foliot";


impl Command {
    /// Execute a command with the given arguments
    fn execute(&self, args: &Args) -> Result<(), String> {
        match self {
            Self::Abort {} => abort(args),
            Self::Clockin { starting } => clockin(*starting, args),
            Self::Clockout { comment } => clockout(comment.clone(), args),
            Self::Clock { hours, starting, comment } => clock_duration(*hours, *starting, comment.clone(), args),
            Self::Edit { clockin } => edit(*clockin, args),
            Self::Git { git_args } => git(git_args, args),
            Self::Path { namespace } => print_path(namespace.clone(), args),
            Self::Show { filter, tail, wrap } => show(filter, *tail, *wrap, args),
            Self::Status {} => status(args),
            Self::Summarize { filter, tail } => summarize(filter, *tail, args),
        }
    }
}

impl ClockinTimestamp {
    /// Creates a [ClockinTimestamp] referencing the date and time of the function call
    fn now() -> Self {
        return ClockinTimestamp { start_time: now() }
    }

    /// Creates a [ClockinTimestamp] referencing a certain starting time
    fn starting(time: &NaiveDateTime) -> Self {
        return ClockinTimestamp { start_time: Local.from_local_datetime(time).unwrap() }
    }

    /// Relative path to the file that contains the last clockin timestamp
    fn relative_path(namespace: &str) -> PathBuf {
        PathBuf::from(format!("{}-clockin", namespace))
            .with_extension("yaml")
    }
}

impl HumanDuration {
    fn zero() -> HumanDuration {
        HumanDuration {
            hours: 0,
            minutes: 0,
        }
    }
}

impl Entry {
    /// Create a new clock entry
    fn create(start_time: DateTime<Local>, end_time: DateTime<Local>, comment: Option<String>) -> Self {
        return Entry {
            start_time, end_time, comment,
        };
    }

    /// Relative path to the file that entries are collected in
    fn relative_path(namespace: &str) -> PathBuf {
        PathBuf::from(format!("{}", namespace))
            .with_extension("yaml")
    }

    /// Total duration of the entry
    fn duration(&self) -> HumanDuration {
        (self.end_time - self.start_time).into()
    }
}

impl Display for HumanDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}h", self.hours, self.minutes)
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Abort {} => write!(f, "abort"),
            Self::Clockin { starting } => match starting {
                Some(time) => write!(f, "clockin --starting \"{}\"", time),
                None => write!(f, "clockin"),
            },
            Self::Clockout { comment } => match comment {
                Some(comment) => write!(f, "clockout \"{}\"", comment),
                None => write!(f, "clockout"),
            },
            Self::Clock { hours, starting, comment } => {
                write!(f, "clock")?;
                if let Some(time) = starting {
                    write!(f, " --starting \"{}\"", time)?;
                }
                write!(f, " {}", hours)?;
                if let Some(comment) = comment {
                    write!(f, " \"{}\"", comment)?;
                }
                write!(f, "")
            },
            Self::Edit { clockin } => match clockin {
                true => write!(f, "edit --clockin"),
                false => write!(f, "edit"),
            },
            Self::Git { git_args } => {
                write!(f, "git")?;
                for arg in git_args {
                    write!(f, "\"{}\"", arg)?;
                }
                write!(f, "")
            },
            Self::Path { namespace } => match namespace {
                Some(ns) => write!(f, "path --namespace \"{}\"", ns),
                None => write!(f, "path"),
            },
            Self::Show { filter, tail, wrap } => {
                write!(f, "show")?;
                if let Some(filter_str) = filter {
                    write!(f, " --filter \"{}\"", filter_str)?;
                }
                write!(f, " --tail {} --wrap {}", tail, wrap)
            },
            Self::Status {} => write!(f, "status"),
            Self::Summarize { filter, tail } => {
                write!(f, "summarize")?;
                if let Some(filter_str) = filter {
                    write!(f, " --filter \"{}\"", filter_str)?;
                }
                write!(f, " --tail {}", tail)
            },
        }
    }
}

impl Add for HumanDuration {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let added_minutes = self.minutes + other.minutes;
        HumanDuration {
            hours: self.hours + other.hours + added_minutes / 60,
            minutes: added_minutes % 60,
        }
    }
}

impl Into<HumanDuration> for chrono::Duration {
    fn into(self) -> HumanDuration {
        HumanDuration {
            hours: self.num_hours(),
            minutes: self.num_minutes() % 60
        }
    }
}

impl Into<TableEntry> for &Entry {
    fn into(self) -> TableEntry {
        TableEntry {
            date: self.start_time.date_naive(),
            from: self.start_time.time(),
            to: self.end_time.time(),
            duration: self.duration(),
            comment: self.comment.clone().unwrap_or(String::new()),
        }
    }
}

impl Into<SummaryTableItem> for (String, Vec<Entry>) {
    fn into(self) -> SummaryTableItem {
        let (month, entries) = self;
        let mut dates: Vec<NaiveDate> = entries.iter().map(|e| e.start_time.date_naive()).collect();
        dates.dedup();

        let total_hours = entries.iter()
            .fold(HumanDuration::zero(), |d, e| d + e.duration());
        let days = dates.len();
        let weeks: f32 = (days_in_month(entries.first().unwrap().start_time.date_naive()) as f32) / 7.0;
        let rem_minutes = if total_hours.minutes == 0 { 0.0 } else { 60.0 / total_hours.minutes as f32 };
        let hours_per_week = ((total_hours.hours as f32) + rem_minutes) / weeks;

        SummaryTableItem {
            month,
            total_hours, days,
            hours_per_week: format!("{:.2}", hours_per_week),
            nitems: entries.len(),
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

fn clock(start: DateTime<Local>, end: DateTime<Local>, comment: Option<String>, args: &Args) -> Result<(), String> {
    let path = Entry::relative_path(&args.namespace);
    let mut entries = if data_file_exists(&path).unwrap() {
        read_data_file(&path)?
    } else {
        Vec::new()
    };

    // Sort entries
    // TODO: Implement Ord/PartialOrd to use only the starting time
    entries.sort();

    let entry = Entry::create(start, end, comment);

    // check if any entry overlaps
    if entries.iter().any(|e| entries_overlap(&entry, e)) {
        return Err("New entry overlaps an existing one".to_owned());
    }

    println!("Adding entry for namespace '{}':", args.namespace);
    println!("\t starting at {}", entry.start_time);
    println!("\t ending at   {}", entry.end_time);
    println!("\t duration:   {}", entry.duration());
    if let Some(comment) = &entry.comment {
        println!("\t comment:    {}", comment);
    }

    entries.push(entry);
    write_data_file(&path, entries)
}

fn clock_duration(hours: f64, starting: Option<NaiveDateTime>, comment: Option<String>, args: &Args)
        -> Result<(), String> {
    let duration = chrono::Duration::minutes((hours * 60.0) as i64);

    let (start, end) = if let Some(starting) = starting {
        let start = Local.from_local_datetime(&starting).unwrap();
        let end = start + duration;
        (start, end)
    } else {
        let end = now();
        let start = end - duration;
        (start, end)
    };

    clock(start, end, comment, args)
}

/// Start a new clock by creating a new clockin file
fn clockin(starting: Option<NaiveDateTime>, args: &Args) -> Result<(), String> {
    let path = ClockinTimestamp::relative_path(&args.namespace);
    let timestamp = if let Some(time) = starting {
        ClockinTimestamp::starting(&time)
    } else {
        ClockinTimestamp::now()
    };

    if data_file_exists(&path).unwrap() {
        return Err(format!("Clock-in file '{}' already exists.\nPlease remove it before continuing.", path.to_string_lossy()));
    }

    println!("Starting clock for namespace '{}' ({})", args.namespace, timestamp.start_time);
    write_data_file(&path, timestamp)
}

/// Stop the clock and entry to the entries file
fn clockout(comment: Option<String>, args: &Args) -> Result<(), String> {
    let clockin_path = ClockinTimestamp::relative_path(&args.namespace);
    let clockin_timestamp: ClockinTimestamp = read_data_file(&clockin_path)
        .map_err(|_| "No clockin file found".to_owned())?;

    clock(clockin_timestamp.start_time, now(), comment, args)?;
    remove_data_file(&clockin_path)
}

fn days_in_month(date: NaiveDate) -> i64 {
    let date_next_month = date.checked_add_months(Months::new(1)).unwrap();
    date_next_month.signed_duration_since(date).num_days()
}

fn edit(clockin: bool, args: &Args) -> Result<(), String> {
    let find_env = |name: &str| env::vars().into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v);

    let editor = find_env("EDITOR")
        .or(find_env("VISUAL"))
        .unwrap_or("vi".to_owned());

    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;
    let path = if clockin {
        let rel_path = ClockinTimestamp::relative_path(&args.namespace);
        xdg_dirs.find_data_file(rel_path)
            .ok_or(format!("No clockin file found for namespace '{}'", args.namespace))?
    } else {
        let rel_path = Entry::relative_path(&args.namespace);
        xdg_dirs.find_data_file(rel_path)
            .ok_or(format!("No entry file found for namespace '{}'", args.namespace))?
    };

    let mut child = process::Command::new(editor)
        .arg(path)
        .spawn()
        .map_err(|_| "Unable to open editor" )?;
    child.wait()
        .map_err(|_| "Editor exited with error code" )?;

    Ok(())
}

/// Check if the timespan of two entries overlap
fn entries_overlap(e1: &Entry, e2: &Entry) -> bool {
    (e1.start_time > e2.start_time && e1.start_time < e2.end_time)
        || (e1.end_time > e2.start_time && e1.end_time < e2.end_time)
        || (e2.start_time > e1.start_time && e2.start_time < e1.end_time)
        || (e2.end_time > e1.start_time && e2.end_time < e1.end_time)
}

/// Run git command in foliot data directory
fn git(git_args: &Vec<String>, _args: &Args) -> Result<(), String> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;
    let working_dir = xdg_dirs.find_data_file("").ok_or(format!("Path not found"))?;
    let path = working_dir.to_str().ok_or(format!("Unable to convert path to string"))?;

    let args: Vec<&str> = iter::once("-C").chain(iter::once(path))
        .chain(git_args.iter().map(|s| s as &str))
        .collect();

    let mut child = process::Command::new("git")
        .args(args)
        .spawn()
        .map_err(|_| "Unable to open git" )?;
    child.wait()
        .map_err(|_| "Git exited with error code" )?;

    Ok(())
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

/// Parse a starting value
fn parse_starting_value(s: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or(NaiveDateTime::parse_from_str(s, "%d.%m.%Y-%H:%M"))
        .or(NaiveDateTime::parse_from_str(s, "%d.%m.%Y %H:%M"))
        .or(parse_starting_value_time(s))
        .map_err(|_| format!("unable to parse datetime '{}'", s))
}

/// Parse a starting datetime based on the time alone (either today or yesterday)
fn parse_starting_value_time(s: &str) -> Result<NaiveDateTime, String> {
    let time = NaiveTime::parse_from_str(s, "%H:%M")
        .or(NaiveTime::parse_from_str(s, "%H:%Mh"))
        .or(NaiveTime::parse_from_str(s, "%H%M"))
        .or(NaiveTime::parse_from_str(s, "%H%Mh"))
        .map_err(|_| format!("unable to parse time '{}'", s))?;

    let current_datetime = now();
    let date = if current_datetime.time() > time {
        current_datetime.date_naive()
    } else {
        current_datetime.date_naive()
            .checked_sub_days(chrono::Days::new(1)).unwrap()
    };

    Ok(NaiveDateTime::new(date, time))
}

/// Print path to foliot data
fn print_path(namespace: Option<String>, _args: &Args) -> Result<(), String> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(XDG_DIR_PREFIX)
        .map_err(|e| e.to_string())?;

    let path = if let Some(namespace) = namespace {
        let rel_path = Entry::relative_path(&namespace);
        let working_dir = xdg_dirs.find_data_file(rel_path).ok_or(format!("Path not found"))?;
        working_dir.to_str().ok_or(format!("Unable to convert path to string"))?
            .to_owned()
    } else {
        let working_dir = xdg_dirs.find_data_file("").ok_or(format!("Path not found"))?;
        working_dir.to_str().ok_or(format!("Unable to convert path to string"))?
            .to_owned()
    };

    println!("{}", path);

    Ok(())
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

/// Print human readable table to the terminal
fn show(filter: &Option<String>, tail: usize, wrap: usize, args: &Args) -> Result<(), String> {
    let path = Entry::relative_path(&args.namespace);
    let mut entries: Vec<Entry> = if data_file_exists(&path).unwrap() {
        read_data_file(&path)?
    } else {
        return Err(format!("No file found for namespace '{}'", args.namespace));
    };
    entries.sort();

    let regex_opt = match filter {
        Some(filter_str) => Some(Regex::new(filter_str).map_err(|e| e.to_string())?),
        None => None,
    };

    let table_entries: Vec<TableEntry> = entries.iter()
        .filter(|&e| regex_opt.as_ref().map_or(true, |re| e.comment.as_ref().map_or(true, |c| re.is_match(&c))))
        .map(|e| e.into())
        .collect();

    let entry_slice = if tail == 0 {
        table_entries.as_slice()
    } else {
        let len = table_entries.len();
        let idx = if len > tail { len - tail } else { 0 };
        &table_entries.as_slice()[idx..]
    };

    let table = Table::new(entry_slice)
        .with(Style::rounded())
        .with(Rows::new(1..).not(Columns::first()).not(Columns::last()).modify().with(Alignment::center()))
        .with(Modify::new(Segment::all()).with(Width::wrap(wrap)))
        .with(Color::FG_GREEN)
        .with(Margin::new(1, 1, 1, 1))
        .to_string();
    println!("{}", table);

    Ok(())
}

fn status(args: &Args) -> Result<(), String> {
    let path = ClockinTimestamp::relative_path(&args.namespace);

    if !data_file_exists(&path).unwrap() {
        println!("Clock is not running for namespace '{}'", args.namespace);
    } else {
        let clockin_timestamp: ClockinTimestamp = read_data_file(&path)?;
        let duration: HumanDuration = (now() - clockin_timestamp.start_time).into();
        println!("Clock running for namespace '{}':", args.namespace);
        println!("\t started {}", clockin_timestamp.start_time);
        println!("\t running {}", duration);
    }

    Ok(())
}

fn summarize(filter: &Option<String>, tail: usize, args: &Args) -> Result<(), String> {
    let path = Entry::relative_path(&args.namespace);
    let mut entries: Vec<Entry> = if data_file_exists(&path).unwrap() {
        read_data_file(&path)?
    } else {
        return Err(format!("No file found for namespace '{}'", args.namespace));
    };
    entries.sort();

    let regex_opt = match filter {
        Some(filter_str) => Some(Regex::new(filter_str).map_err(|e| e.to_string())?),
        None => None,
    };

    if let Some(re) = regex_opt {
        entries = entries.into_iter()
            .filter(|e| e.comment.as_ref().map_or(true, |c| re.is_match(&c)))
            .collect();
    }

    let mut entries_by_month: HashMap<String, Vec<Entry>> = HashMap::new();

    for entry in entries {
        // for now adding the month number insures correct sorting
        let month = entry.start_time.format("%Y/%m %B").to_string();

        if let Some(month_vec) = entries_by_month.get_mut(&month) {
            month_vec.push(entry);
        } else {
            let month_vec = vec![entry];
            entries_by_month.insert(month, month_vec);
        }
    }

    let mut table_items: Vec<SummaryTableItem> = entries_by_month.drain()
        .map(|m| m.into())
        .collect();
    table_items.sort();

    let items_slice = if tail == 0 {
        table_items.as_slice()
    } else {
        let len = table_items.len();
        let idx = if len > tail { len - tail } else { 0 };
        &table_items.as_slice()[idx..]
    };

    let table = Table::new(items_slice)
        .with(Style::rounded())
        .with(Rows::new(1..).not(Columns::first()).modify().with(Alignment::center()))
        .with(Color::FG_GREEN)
        .with(Margin::new(1, 1, 1, 1))
        .to_string();
    println!("{}", table);


    Ok(())
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

    if args.git_commit {
        let message = format!("[{}] {}", args.namespace, command);
        println!("\n=> git commit -am \"{}\"", message);
        if let Err(e) = git(&vec!["commit".to_owned(), "-am".to_owned(), message], &args) {
            println!("Error: {}", e);
            std::process::exit(1);
        }

        if args.git_push {
            println!("\n=> git pull --rebase");
            if let Err(e) = git(&vec!["pull".to_owned(), "--rebase".to_owned()], &args) {
                println!("Error: {}", e);
                std::process::exit(1);
            }

            println!("\n=> git push");
            if let Err(e) = git(&vec!["push".to_owned()], &args) {
                println!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

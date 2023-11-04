extern crate chrono;
extern crate clap;
extern crate rusqlite;
use chrono::{DateTime, Duration, Utc};
use clap::{arg, Command};
use rusqlite::{params, Connection, Result};


#[derive(Debug)]
struct Entry {
    id: i32,
    task: String,
    in_time: DateTime<Utc>,
    out_time: Option<DateTime<Utc>>,
    billed: String,
}

fn cli() -> Command {
    Command::new("punch")
        .about("Time tracking")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .subcommand(
            Command::new("in")
                .about("Punch into a task")
                .arg(arg!(<TASK> "The task name"))
                .arg_required_else_help(true),
        )
        .subcommand(Command::new("out").about("Punch out from the current task"))
        .subcommand(
            Command::new("list")
                .about("list")
                .arg(arg!(<TASK> "List date and time for given task"))
                .arg(arg!(--"billed")),
        )
}

fn main() -> Result<()> {
    let matches = cli().get_matches();

    let path = expand_home(".punch.db")?;

    let conn = Connection::open(&path)?;

    let table_exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='tasks'",
            [],
            |_| Ok(()),
        )
        .is_ok();

    if !table_exists {
        println!("Initializing ~/.punch.db");
        // Initialize the database if it doesn't exist
        conn.execute(
            "CREATE TABLE tasks (
                id INTEGER PRIMARY KEY,
                task TEXT,
                intime TEXT NOT NULL,
                outtime TEXT,
                billed BOOLEAN DEFAULT 'n' NOT NULL
            )",
            [],
        )?;
    }

    match matches.subcommand() {
        Some(("in", sub_m)) => {
            let task = sub_m.get_one::<String>("TASK").expect("required");
            let rows = conn.query_row(
                "SELECT id FROM tasks WHERE outtime IS NULL ORDER BY intime DESC LIMIT 1",
                [],
                |_row| Ok(()),
            );

            match rows {
                Ok(_i) => println!("Can't punch in again"),
                Err(_) => {
                    println!("Punching into {}", task);
                    conn.execute(
                        "INSERT INTO tasks (task, intime, outtime) VALUES (?, ?, NULL)",
                        &[&task, &Utc::now().to_rfc3339()],
                    )?;
                }
            }
        }
        Some(("out", _)) => {
            let mut q = conn.prepare(
                "SELECT id, task, intime, outtime, billed FROM tasks WHERE outtime IS NULL ORDER BY intime DESC LIMIT 1")?;
            let row = q.query_map(params![], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    task: row.get(1)?,
                    in_time: row.get(2)?,
                    out_time: row.get(3)?,
                    billed: row.get(4)?,
                })
            });

            match row {
                Ok(mut e) => {
                    if let Some(entry) = e.next() {
                        let entry = entry.expect("failed");
                        let now = Utc::now();
                        let duration = now.signed_duration_since(entry.in_time);
                        conn.execute(
                            "UPDATE tasks SET outtime = ? WHERE id = ?",
                            &[&now.to_rfc3339(), &entry.id.to_string()],
                        )?;
                        println!(
                            "Punched out from '{}' after {} minutes",
                            entry.task,
                            duration.num_minutes()
                        );
                    } else {
                        println!("Can't punch out if you're not in...");
                    }
                }
                Err(_) => println!("Can't punch out if you're not in..."),
            }
        }
        Some(("list", sub_m)) => {
            let task: &String = sub_m.get_one::<String>("TASK").expect("required");
            let billed_flag = sub_m.try_get_one::<bool>("--billed");
            // also show billed entries
            let show_billed = match billed_flag.unwrap_or(Some(&false)) {
                Some(true) => {
                    println!("showing billed entries");
                    true
                }
                _ => false,
            };

            let mut statement = conn.prepare("SELECT * FROM tasks WHERE task = ?")?;
            let entry_iter = statement.query_map(params![task], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    task: row.get(1)?,
                    in_time: row.get(2)?,
                    out_time: row.get(3)?,
                    billed: row.get(4)?,
                })
            })?;
            let entry_vec: Vec<_> = entry_iter.collect();
            if entry_vec.is_empty() {
                println!("Nothing for task '{}'", task);
            } else {
                let mut total = chrono::Duration::zero();
                for e in entry_vec {
                    let entry = e.unwrap();
                    let duration: Duration = match entry.out_time {
                        Some(out_time) => {
                            let duration = out_time.signed_duration_since(entry.in_time);
                            duration
                        }
                        None => chrono::Duration::zero(),
                    };
                    if entry.billed == "y" && !show_billed {
                        continue;
                    } else {
                        total = total + duration;
                        println!(
                            "{}\n  Date: {}\n  Duration: {}\n  Billed: {}",
                            entry.task,
                            entry.in_time.format("%Y-%m-%d"),
                            display_mins(duration),
                            from_billed(entry.billed)
                        );
                    }
                }
                println!("Total: {}", display_mins(total));
            }
        }
        Some((s, _)) => {
            println!("unknown {:?}", &s)
        }
        None => {
            println!("none");
        }
    }

    Ok(())
}

fn from_billed(s: String) -> String {
    if s == "n" {
        return "No".to_string();
    } else {
        return "Yes".to_string();
    }
}
fn expand_home(path: &str) -> Result<String, rusqlite::Error> {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        return Ok(format!("{}/{}", home_str, path));
    }
    Err(rusqlite::Error::InvalidParameterName(
        "home error".to_string(),
    ))
}

fn display_mins(duration: Duration) -> String {
    let minutes = duration.num_minutes();
    if minutes < 60 {
        return format!("{} minutes", minutes);
    } else {
        let hours = minutes / 60;
        let remaining_minutes = minutes % 60;
        return format!("{} hours {} minutes", hours, remaining_minutes);
    }
}

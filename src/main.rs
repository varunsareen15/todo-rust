mod tui;

use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io,
    path::Path,
};
use tui::Todo as TuiTodo;
use std::io::Write;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime};
use chrono::format::ParseError;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Todo {
    id: usize,
    text: String,
    done: bool,
    due_date: Option<String>,  // ISO 8601 format: YYYY-MM-DD
    reminder: Option<String>,  // ISO 8601 format: YYYY-MM-DD HH:MM
}

#[derive(Parser)]
#[command(name = "todo")]
#[command(about = "A todo CLI app in Rust")]
struct Cli {
    #[arg(long, help = "Use SQLite instead of JSON")]
    sqlite: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new todo item
    Add { 
        /// The text content of the todo
        text: Vec<String> 
    },
    /// Mark a todo as done
    Done { 
        /// The ID of the todo to mark as done
        id: usize 
    },
    /// Edit a todo's text content
    Edit { 
        /// The ID of the todo to edit
        id: usize 
    },
    /// Delete a todo
    Delete { 
        /// The ID of the todo to delete
        id: usize 
    },
    /// List all todos
    List,
    /// Open the interactive terminal user interface
    Tui,
    /// Set a due date for a todo
    Due { 
        /// The ID of the todo
        id: usize,
        /// Due date in YYYY-MM-DD format
        date: String 
    },
    /// Set a reminder for a todo
    Remind { 
        /// The ID of the todo
        id: usize,
        /// Date in YYYY-MM-DD format
        date: String,
        /// Time in HH:MM format (24-hour)
        time: String 
    },
    /// List upcoming reminders
    Upcoming,
    /// Clear a reminder from a todo
    ClearReminder {
        /// The ID of the todo
        id: usize
    },
}

const FILE_PATH: &str = "/home/varun/Projects/todo/todos.json";

fn main() {
    let cli = Cli::parse();

    if cli.sqlite {
        let mut conn = init_db();
        handle_sqlite_commands(&mut conn, cli.command);
    } else {
        let mut todos = load_todos();
        handle_json_commands(cli.command, &mut todos);
        save_todos(&todos).unwrap();
    }
}

fn format_todo(todo: &Todo) -> String {
    let status = if todo.done { "✓" } else { " " };
    let due_date = todo.due_date.as_deref().unwrap_or("No due date");
    let reminder = todo.reminder.as_deref().unwrap_or("No reminder");
    format!("[{}] {}: {} (Due: {}, Reminder: {})", status, todo.id, todo.text, due_date, reminder)
}

fn handle_json_commands(cmd: Commands, todos: &mut Vec<Todo>) {
    match cmd {
        Commands::Add { text } => {
            let id = todos.len() + 1;
            let joined = text.join(" ");
            todos.push(Todo {
                id,
                text: joined,
                done: false,
                due_date: None,
                reminder: None,
            });
            println!("✅ Todo added!");
        }
        Commands::Done { id } => {
            if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                todo.done = true;
                println!("🎉 Todo marked as done!");
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::Edit { id } => {
            if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                let tmp_path = "/tmp/todo_edit.txt";
                fs::write(tmp_path, &todo.text).expect("Failed to write temp file");

                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
                std::process::Command::new(editor)
                    .arg(tmp_path)
                    .status()
                    .expect("Failed to open editor");

                let updated_text = fs::read_to_string(tmp_path).expect("Failed to read file");
                todo.text = updated_text.trim().to_string();
                println!("📝 Todo updated!");
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::Delete { id } => {
            let len_before = todos.len();
            todos.retain(|todo| todo.id != id);
            for (i, todo) in todos.iter_mut().enumerate() {
                todo.id = i + 1;
            }
            if todos.len() < len_before {
                println!("🗑️ Deleted todo with id {}", id);
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::List => {
            for todo in todos.iter() {
                println!("{}", format_todo(todo));
            }
        }
        Commands::Tui => {
            handle_tui_command_json(todos);
        }
        Commands::Due { id, date } => {
            match validate_date(&date) {
                Ok(_) => {
                    if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                        todo.due_date = Some(date);
                        println!("📅 Due date set for todo {}!", id);
                    } else {
                        eprintln!("❌ Todo with id {} not found", id);
                    }
                }
                Err(_) => eprintln!("❌ Invalid date format. Please use YYYY-MM-DD"),
            }
        }
        Commands::Remind { id, date, time } => {
            match validate_datetime(&date, &time) {
                Ok(datetime) => {
                    if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                        todo.reminder = Some(format_datetime(&datetime));
                        println!("⏰ Reminder set for todo {}!", id);
                    } else {
                        eprintln!("❌ Todo with id {} not found", id);
                    }
                }
                Err(_) => eprintln!("❌ Invalid date/time format. Please use YYYY-MM-DD HH:MM"),
            }
        }
        Commands::Upcoming => {
            let now = Local::now().naive_local();
            let mut upcoming: Vec<_> = todos.iter()
                .filter(|todo| !todo.done)
                .filter_map(|todo| {
                    todo.reminder.as_ref().and_then(|reminder| {
                        NaiveDateTime::parse_from_str(reminder, "%Y-%m-%d %H:%M")
                            .ok()
                            .map(|dt| (todo, dt))
                    })
                })
                .filter(|(_, dt)| *dt > now)
                .collect();
            
            upcoming.sort_by(|(_, a), (_, b)| a.cmp(b));
            
            if upcoming.is_empty() {
                println!("No upcoming reminders");
            } else {
                println!("Upcoming reminders:");
                for (todo, dt) in upcoming {
                    println!("[{}] {} - Due: {}", todo.id, todo.text, format_datetime(&dt));
                }
            }
        }
        Commands::ClearReminder { id } => {
            if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                todo.reminder = None;
                println!("🗑️ Reminder cleared for todo {}!", id);
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
    }
}

fn handle_sqlite_commands(conn: &mut Connection, cmd: Commands) {
    match cmd {
        Commands::Add { text } => {
            let joined = text.join(" ");
            conn.execute("INSERT INTO todos (text, done) VALUES (?1, 0)", params![joined])
                .unwrap();
            println!("✅ Todo added (SQLite)!");
        }
        Commands::Done { id } => {
            let affected = conn
                .execute("UPDATE todos SET done = 1 WHERE id = ?1", params![id])
                .unwrap();
            if affected > 0 {
                println!("🎉 Todo marked as done (SQLite)!");
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::Edit { id } => {
            let mut stmt = conn
                .prepare("SELECT text FROM todos WHERE id = ?1")
                .unwrap();
            let mut rows = stmt.query(params![id]).unwrap();
            if let Some(row) = rows.next().unwrap() {
                let current_text: String = row.get(0).unwrap();
                let tmp_path = "/tmp/todo_sqlite_edit.txt";
                fs::write(tmp_path, &current_text).unwrap();

                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
                std::process::Command::new(editor)
                    .arg(tmp_path)
                    .status()
                    .expect("Failed to open editor");

                let new_text = fs::read_to_string(tmp_path).unwrap();
                conn.execute(
                    "UPDATE todos SET text = ?1 WHERE id = ?2",
                    params![new_text.trim(), id],
                )
                .unwrap();
                println!("📝 Todo updated (SQLite)!");
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::Delete { id } => {
            let affected = conn
                .execute("DELETE FROM todos WHERE id = ?1", params![id])
                .unwrap();
            if affected > 0 {
                println!("🗑️ Deleted todo with id {} (SQLite)", id);
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::List => {
            let todos = load_todos_from_sqlite(conn);
            for todo in todos {
                println!("{}", format_todo(&todo));
            }
        }
        Commands::Tui => {
            let todos = load_todos_from_sqlite(conn);
            let todos_for_tui: Vec<TuiTodo> = todos
                .iter()
                .map(|t| TuiTodo {
                    id: t.id,
                    text: t.text.clone(),
                    done: t.done,
                    due_date: t.due_date.clone(),
                    reminder: t.reminder.clone(),
                })
                .collect();

            match tui::run_tui(todos_for_tui) {
                Ok(updated_todos) => {
                    let todos: Vec<Todo> = updated_todos
                        .into_iter()
                        .enumerate()
                        .map(|(i, t)| Todo {
                            id: i + 1,
                            text: t.text,
                            done: t.done,
                            due_date: t.due_date,
                            reminder: t.reminder,
                        })
                        .collect();

                    save_todos_to_sqlite(conn, &todos);
                }
                Err(e) => eprintln!("TUI Error: {}", e),
            }
        }
        Commands::Due { id, date } => {
            match validate_date(&date) {
                Ok(_) => {
                    let affected = conn
                        .execute("UPDATE todos SET due_date = ?1 WHERE id = ?2", params![date, id])
                        .unwrap();
                    if affected > 0 {
                        println!("📅 Due date set for todo {} (SQLite)!", id);
                    } else {
                        eprintln!("❌ Todo with id {} not found", id);
                    }
                }
                Err(_) => eprintln!("❌ Invalid date format. Please use YYYY-MM-DD"),
            }
        }
        Commands::Remind { id, date, time } => {
            match validate_datetime(&date, &time) {
                Ok(datetime) => {
                    let datetime_str = format_datetime(&datetime);
                    let affected = conn
                        .execute("UPDATE todos SET reminder = ?1 WHERE id = ?2", params![datetime_str, id])
                        .unwrap();
                    if affected > 0 {
                        println!("⏰ Reminder set for todo {} (SQLite)!", id);
                    } else {
                        eprintln!("❌ Todo with id {} not found", id);
                    }
                }
                Err(_) => eprintln!("❌ Invalid date/time format. Please use YYYY-MM-DD HH:MM"),
            }
        }
        Commands::Upcoming => {
            let todos = load_todos_from_sqlite(conn);
            let now = Local::now().naive_local();
            let mut upcoming: Vec<_> = todos.iter()
                .filter(|todo| !todo.done)
                .filter_map(|todo| {
                    todo.reminder.as_ref().and_then(|reminder| {
                        NaiveDateTime::parse_from_str(reminder, "%Y-%m-%d %H:%M")
                            .ok()
                            .map(|dt| (todo, dt))
                    })
                })
                .filter(|(_, dt)| *dt > now)
                .collect();
            
            upcoming.sort_by(|(_, a), (_, b)| a.cmp(b));
            
            if upcoming.is_empty() {
                println!("No upcoming reminders");
            } else {
                println!("Upcoming reminders:");
                for (todo, dt) in upcoming {
                    println!("[{}] {} - Due: {}", todo.id, todo.text, format_datetime(&dt));
                }
            }
        }
        Commands::ClearReminder { id } => {
            let affected = conn
                .execute("UPDATE todos SET reminder = NULL WHERE id = ?1", params![id])
                .unwrap();
            if affected > 0 {
                println!("🗑️ Reminder cleared for todo {} (SQLite)!", id);
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
    }
}

fn handle_tui_command_json(todos: &mut Vec<Todo>) {
    let todos_for_tui: Vec<TuiTodo> = todos
        .iter()
        .map(|t| TuiTodo {
            id: t.id,
            text: t.text.clone(),
            done: t.done,
            due_date: t.due_date.clone(),
            reminder: t.reminder.clone(),
        })
        .collect();

    match tui::run_tui(todos_for_tui) {
        Ok(updated_todos) => {
            todos.clear();
            for (i, t) in updated_todos.into_iter().enumerate() {
                todos.push(Todo {
                    id: i + 1,
                    text: t.text,
                    done: t.done,
                    due_date: t.due_date,
                    reminder: t.reminder,
                });
            }
            save_todos(todos).unwrap();
        }
        Err(e) => eprintln!("TUI Error: {}", e),
    }
}

fn load_todos_from_sqlite(conn: &Connection) -> Vec<Todo> {
    let mut stmt = conn
        .prepare("SELECT id, text, done, due_date, reminder FROM todos ORDER BY id ASC")
        .unwrap();

    let rows = stmt
        .query_map([], |row| {
            Ok(Todo {
                id: row.get(0)?,
                text: row.get(1)?,
                done: row.get(2)?,
                due_date: row.get(3)?,
                reminder: row.get(4)?,
            })
        })
        .unwrap();

    rows.filter_map(Result::ok).collect()
}

fn save_todos_to_sqlite(conn: &mut Connection, todos: &[Todo]) {
    let tx = conn.transaction().unwrap();
    tx.execute("DELETE FROM todos", []).unwrap();

    for todo in todos {
        tx.execute(
            "INSERT INTO todos (id, text, done, due_date, reminder) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![todo.id, todo.text, todo.done, todo.due_date, todo.reminder],
        )
        .unwrap();
    }

    tx.commit().unwrap();
}

fn init_db() -> Connection {
    let conn = Connection::open("todos.db").unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY,
            text TEXT NOT NULL,
            done BOOLEAN NOT NULL DEFAULT 0,
            due_date TEXT,
            reminder TEXT
        )",
        [],
    )
    .unwrap();
    conn
}

fn load_todos() -> Vec<Todo> {
    if !Path::new(FILE_PATH).exists() {
        return vec![];
    }
    let data = fs::read_to_string(FILE_PATH).unwrap_or_default();
    serde_json::from_str(&data).unwrap_or_else(|_| vec![])
}

fn save_todos(todos: &Vec<Todo>) -> io::Result<()> {
    let json = serde_json::to_string_pretty(todos)?;
    let mut file = File::create(FILE_PATH)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

fn validate_date(date_str: &str) -> Result<NaiveDate, ParseError> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
}

fn validate_time(time_str: &str) -> Result<NaiveTime, ParseError> {
    NaiveTime::parse_from_str(time_str, "%H:%M")
}

fn validate_datetime(date_str: &str, time_str: &str) -> Result<NaiveDateTime, ParseError> {
    let date = validate_date(date_str)?;
    let time = validate_time(time_str)?;
    Ok(NaiveDateTime::new(date, time))
}

fn format_datetime(dt: &NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}


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
    Add { text: Vec<String> },
    Done { id: usize },
    Edit { id: usize },
    Delete { id: usize },
    List,
    Tui,
    Due { id: usize, date: String },
    Remind { id: usize, datetime: String },
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
            if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                todo.due_date = Some(date);
                println!("📅 Due date set for todo {}!", id);
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::Remind { id, datetime } => {
            if let Some(todo) = todos.iter_mut().find(|t| t.id == id) {
                todo.reminder = Some(datetime);
                println!("⏰ Reminder set for todo {}!", id);
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
                            due_date: None,
                            reminder: None,
                        })
                        .collect();

                    save_todos_to_sqlite(conn, &todos);
                }
                Err(e) => eprintln!("TUI Error: {}", e),
            }
        }
        Commands::Due { id, date } => {
            let affected = conn
                .execute("UPDATE todos SET due_date = ?1 WHERE id = ?2", params![date, id])
                .unwrap();
            if affected > 0 {
                println!("📅 Due date set for todo {} (SQLite)!", id);
            } else {
                eprintln!("❌ Todo with id {} not found", id);
            }
        }
        Commands::Remind { id, datetime } => {
            let affected = conn
                .execute("UPDATE todos SET reminder = ?1 WHERE id = ?2", params![datetime, id])
                .unwrap();
            if affected > 0 {
                println!("⏰ Reminder set for todo {} (SQLite)!", id);
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
                    due_date: None,
                    reminder: None,
                });
            }
            save_todos(todos).unwrap();
        }
        Err(e) => eprintln!("TUI Error: {}", e),
    }
}

fn load_todos_from_sqlite(conn: &Connection) -> Vec<Todo> {
    let mut stmt = conn
        .prepare("SELECT id, text, done FROM todos ORDER BY id ASC")
        .unwrap();

    let rows = stmt
        .query_map([], |row| {
            Ok(Todo {
                id: row.get(0)?,
                text: row.get(1)?,
                done: row.get(2)?,
                due_date: None,
                reminder: None,
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
            "INSERT INTO todos (id, text, done) VALUES (?1, ?2, ?3)",
            params![todo.id, todo.text, todo.done],
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


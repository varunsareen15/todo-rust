use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    fs,
    io::{self, Write},
    time::Duration,
};
use tui::{
    backend::{CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
};

#[derive(Clone)]
pub struct Todo {
    pub id: usize,
    pub text: String,
    pub done: bool,
    pub due_date: Option<String>,
    pub reminder: Option<String>,
}

fn format_todo(todo: &Todo) -> String {
    let status = if todo.done { "[x]" } else { "[ ]" };
    let due_date = todo.due_date.as_deref().unwrap_or("No due date");
    let reminder = todo.reminder.as_deref().unwrap_or("No reminder");
    format!("{} {} (Due: {}, Reminder: {})", status, todo.text, due_date, reminder)
}

pub fn run_tui(mut todos: Vec<Todo>) -> Result<Vec<Todo>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut selected = 0;

    loop {
        terminal.draw(|f| {
            let size = f.size();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
                .split(size);

            let title_block = Block::default()
                .borders(Borders::ALL)
                .title(Spans::from(vec![Span::styled(
                    "↑↓ move • Space toggle • a add • e edit • d delete • t due date • r reminder • c clear reminder • q quit",
                    Style::default().fg(Color::Yellow),
                )]));

            let items: Vec<ListItem> = todos
                .iter()
                .map(|t| {
                    ListItem::new(vec![Spans::from(Span::raw(format_todo(t)))])
                })
                .collect();

            let mut state = ListState::default();
            state.select(Some(selected));

            let list = List::new(items)
                .block(title_block)
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, chunks[1], &mut state);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down => {
                        if selected < todos.len().saturating_sub(1) {
                            selected += 1;
                        }
                    }
                    KeyCode::Up => {
                        if selected > 0 {
                            selected -= 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(todo) = todos.get_mut(selected) {
                            todo.done = !todo.done;
                        }
                    }
                    KeyCode::Char('d') => {
                        if selected < todos.len() {
                            todos.remove(selected);
                            if selected > 0 {
                                selected -= 1;
                            }
                        }
                    }
                    KeyCode::Char('e') => {
                        if let Some(todo) = todos.get_mut(selected) {
                            let tmp_path = "/tmp/todo_edit.txt";
                            fs::write(tmp_path, &todo.text)?;

                            if run_editor(tmp_path, &mut terminal).is_ok() {
                                let updated = fs::read_to_string(tmp_path)?;
                                if !updated.trim().is_empty() {
                                    todo.text = updated.trim().to_string();
                                }
                            }
                        }
                    }
                    KeyCode::Char('t') => {
                        if let Some(todo) = todos.get_mut(selected) {
                            let tmp_path = "/tmp/todo_due.txt";
                            fs::write(tmp_path, todo.due_date.as_deref().unwrap_or(""))?;

                            if run_editor(tmp_path, &mut terminal).is_ok() {
                                let updated = fs::read_to_string(tmp_path)?;
                                let updated = updated.trim();
                                if !updated.is_empty() {
                                    todo.due_date = Some(updated.to_string());
                                } else {
                                    todo.due_date = None;
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(todo) = todos.get_mut(selected) {
                            let tmp_path = "/tmp/todo_reminder.txt";
                            fs::write(tmp_path, todo.reminder.as_deref().unwrap_or(""))?;

                            if run_editor(tmp_path, &mut terminal).is_ok() {
                                let updated = fs::read_to_string(tmp_path)?;
                                let updated = updated.trim();
                                if !updated.is_empty() {
                                    todo.reminder = Some(updated.to_string());
                                } else {
                                    todo.reminder = None;
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        if let Some(todo) = todos.get_mut(selected) {
                            todo.reminder = None;
                        }
                    }
                    KeyCode::Char('a') => {
                        let tmp_path = "/tmp/todo_new.txt";
                        fs::write(tmp_path, "")?;

                        if run_editor(tmp_path, &mut terminal).is_ok() {
                            let new_text = fs::read_to_string(tmp_path)?;
                            let new_text = new_text.trim();
                            if !new_text.is_empty() {
                                todos.push(Todo {
                                    id: todos.len() + 1,
                                    text: new_text.to_string(),
                                    done: false,
                                    due_date: None,
                                    reminder: None,
                                });
                                selected = todos.len().saturating_sub(1);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(todos)
}

/// Temporarily leave TUI to run $EDITOR and refresh screen after
fn run_editor(
    temp_file: &str,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let status = std::process::Command::new(editor).arg(temp_file).status();

    // Restore screen
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )?;
    enable_raw_mode()?;

    // Redraw screen immediately
    terminal.draw(|_| {})?;

    match status {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}


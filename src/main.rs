mod api;
mod app;
mod config;
mod markdown;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

use app::AppMode;

#[tokio::main]
async fn main() -> Result<()> {
    let api_key = config::load_api_key("ANTHROPIC_API_KEY");

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;

    let result = run(app::App::new(api_key)).await;

    disable_raw_mode().ok();
    execute!(io::stdout(), LeaveAlternateScreen).ok();

    result
}

async fn run(mut app: app::App) -> Result<()> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        app.poll_stream();
        terminal.draw(|f| ui::render(f, &mut app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => match &app.mode {
                    AppMode::Normal => match (key.modifiers, key.code) {
                        (KeyModifiers::CONTROL, KeyCode::Char('c'))
                        | (KeyModifiers::CONTROL, KeyCode::Char('d')) => break,

                        (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                            if app.streaming {
                                app.cancel_stream();
                            }
                        }

                        // Ctrl+P opens model picker (Ctrl+M = carriage return in terminals)
                        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                            app.open_model_picker();
                        }

                        // Plain Enter submits; Alt+Enter inserts a newline
                        (KeyModifiers::NONE, KeyCode::Enter) => app.submit(),
                        (KeyModifiers::ALT, KeyCode::Enter) => {
                            app.textarea.insert_newline();
                        }

                        (KeyModifiers::NONE, KeyCode::Up)
                        | (KeyModifiers::NONE, KeyCode::PageUp) => app.scroll_up(),

                        (KeyModifiers::NONE, KeyCode::Down)
                        | (KeyModifiers::NONE, KeyCode::PageDown) => app.scroll_down(),

                        _ => {
                            app.textarea.input(key);
                        }
                    },
                    AppMode::ModelSelect => match key.code {
                        KeyCode::Esc => app.close_model_picker(),
                        KeyCode::Enter => app.confirm_model_select(),
                        KeyCode::Up | KeyCode::Char('k') => app.picker_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.picker_down(),
                        _ => {}
                    },
                },
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}

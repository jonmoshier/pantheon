mod api;
mod app;
mod config;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

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
            if let Event::Key(key) = event::read()? {
                match (key.modifiers, key.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('c'))
                    | (KeyModifiers::CONTROL, KeyCode::Char('d')) => break,

                    (KeyModifiers::NONE, KeyCode::Enter) => app.submit(),
                    (KeyModifiers::NONE, KeyCode::Backspace) => app.delete_back(),
                    (KeyModifiers::NONE, KeyCode::Left) => app.move_left(),
                    (KeyModifiers::NONE, KeyCode::Right) => app.move_right(),
                    (KeyModifiers::NONE, KeyCode::Home) => app.move_home(),
                    (KeyModifiers::NONE, KeyCode::End) => app.move_end(),

                    (KeyModifiers::NONE, KeyCode::Up)
                    | (KeyModifiers::NONE, KeyCode::PageUp) => app.scroll_up(),
                    (KeyModifiers::NONE, KeyCode::Down)
                    | (KeyModifiers::NONE, KeyCode::PageDown) => app.scroll_down(),

                    (_, KeyCode::Char(c)) => app.insert_char(c),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

mod controller;
mod model;
mod view;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use std::io::StdoutLock;
use std::{env, error::Error, fs, io};
use tui::backend::CrosstermBackend;
use tui::Terminal;

use crate::model::config::Config;
use crate::model::App;

fn main() -> Result<(), Box<dyn Error>> {
    let file_path = env::args()
        .nth(1)
        .or_else(|| env::var("SERVICE_MANAGER_CONFIG_PATH").ok())
        .unwrap_or_else(|| {
            eprintln!(
                "error: config file not found
usage: <filepath>
or use the environment variable SERVICE_MANAGER_CONFIG_PATH"
            );
            std::process::exit(1);
        });

    let config_file = fs::read(file_path).unwrap_or_else(|e| {
        eprintln!("error: failed to read file: {}", e);
        std::process::exit(1);
    });

    let config = toml::from_slice::<Config>(&config_file).unwrap_or_else(|e| {
        eprintln!("error: invalid config file: {}", e);
        std::process::exit(1);
    });

    let stdout = io::stdout();
    let stdout = stdout.lock();

    let mut terminal = setup_terminal(stdout)?;

    // create app and run it
    let app = App::new(config);
    let res = controller::run_app(&mut terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn setup_terminal(mut stdout: StdoutLock) -> io::Result<Terminal<CrosstermBackend<StdoutLock>>> {
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

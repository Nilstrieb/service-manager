mod controller;
mod model;
mod view;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use std::io::{ErrorKind, Read, StdoutLock};
use std::process::Stdio;
use std::time::Duration;
use std::{env, fs, io};
use tui::backend::CrosstermBackend;
use tui::Terminal;

use crate::model::config::Config;
use crate::model::App;

fn main() {
    /////  ------------------------ TEST START ------------------------

    let mut cmd = std::process::Command::new("sh");

    let mut child = cmd
        .stdout(Stdio::piped())
        .args(&["-c", "echo hello"])
        .spawn()
        .unwrap();

    let mut stdout = child.stdout.take().unwrap(); // std::fs::File::open("config.toml").unwrap();
    std::thread::spawn(move || {
        let mut buf = [0; 1024];

        println!("{}", buf.len());

        // stdout.read_to_end(&mut buf);

        // println!("{}", String::from_utf8_lossy(&buf).as_ref());

        // return;

        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => println!("READ: now `{}`", String::from_utf8(buf.to_vec()).unwrap()),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => eprintln!("error: {}", e),
            }
        }
        println!("done!");
    });

    std::thread::sleep(Duration::from_secs(3454233));

    /////  ------------------------  TEST END  ------------------------

    let file_path = env::args()
        .nth(1)
        .or_else(|| env::var("SERVICE_MANAGER_CONFIG_PATH").ok())
        .or_else(|| Some("config.toml".to_string()))
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

    let mut terminal = setup_terminal(stdout).expect("failed to setup terminal");

    // create app and run it
    let app = App::new(config);

    if let Ok(app) = app {
        let res = controller::run_app(&mut terminal, app);

        if let Err(err) = res {
            println!("{:?}", err)
        }
    }

    // restore terminal

    if let Err(e) = disable_raw_mode() {
        eprintln!("error: {}", e);
    }

    if let Err(e) = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ) {
        eprintln!("error: {}", e);
    }

    if let Err(e) = terminal.show_cursor() {
        eprintln!("error: {}", e);
    }
}

fn setup_terminal(mut stdout: StdoutLock) -> io::Result<Terminal<CrosstermBackend<StdoutLock>>> {
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

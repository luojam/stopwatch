mod client;
mod daemon;
mod protocol;
mod stopwatch;

use std::{io, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};

use protocol::{Command, State};

#[derive(Parser)]
#[command(version, about = "A stopwatch for your terminal")]
struct Args {
    /// Use this socket path instead of the default
    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Run the daemon in the foreground
    Daemon,
    /// Show the timer's current state and elapsed time
    Status,
    /// Start or resume the timer
    Start,
    /// Pause the timer, keeping its elapsed time
    Pause,
    /// Clear the elapsed time and pause the timer
    Reset,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stopwatch: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let args = Args::parse();
    let path = match args.socket {
        Some(path) => path,
        None => default_socket()?,
    };

    let command = match args.command {
        Action::Daemon => return daemon::run(&path),
        Action::Status => Command::Status,
        Action::Start => Command::Start,
        Action::Pause => Command::Pause,
        Action::Reset => Command::Reset,
    };

    let status = client::request(&path, command)?;
    let state = match status.state {
        State::Paused => "paused",
        State::Running => "running",
    };
    println!("{state} {}ms", status.elapsed_ms);
    Ok(())
}

fn default_socket() -> io::Result<PathBuf> {
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            io::Error::other("set XDG_RUNTIME_DIR to an absolute path or use --socket PATH")
        })?;

    Ok(directory.join("stopwatch/stopwatch.sock"))
}

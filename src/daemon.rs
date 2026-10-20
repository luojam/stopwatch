use std::{
    fs,
    io::{self, BufReader},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::Path,
};

use crate::{
    protocol::{self, Command, IO_TIMEOUT, Response, State, Status},
    stopwatch::Stopwatch,
};

pub fn run(path: &Path) -> io::Result<()> {
    let listener = bind(path)?;
    let mut timer = Stopwatch::default();
    eprintln!("Listening on {}", path.display());

    for connection in listener.incoming() {
        let stream = match connection {
            Ok(stream) => stream,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };

        if let Err(error) = serve(stream, &mut timer) {
            eprintln!("client connection failed: {error}");
        }
    }

    Ok(())
}

fn bind(path: &Path) -> io::Result<UnixListener> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }

    let listener = UnixListener::bind(path).map_err(|error| {
        let message = if error.kind() == io::ErrorKind::AddrInUse {
            format!(
                "cannot bind {}: path already exists; remove it only after confirming the daemon has stopped",
                path.display()
            )
        } else {
            format!("cannot bind {}: {error}", path.display())
        };
        io::Error::new(error.kind(), message)
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;

    Ok(listener)
}

fn serve(stream: UnixStream, timer: &mut Stopwatch) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut connection = BufReader::new(stream);

    let command = match protocol::read_message(&mut connection) {
        Ok(Some(command)) => command,
        Ok(None) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return protocol::write_message(
                connection.get_mut(),
                &Response::Error {
                    error: error.to_string(),
                },
            );
        }
        Err(error) => return Err(error),
    };

    match command {
        Command::Status => {}
        Command::Start => timer.start(),
        Command::Pause => timer.pause(),
        Command::Reset => timer.reset(),
    }

    let response = Response::Status(Status {
        state: if timer.is_running() {
            State::Running
        } else {
            State::Paused
        },
        elapsed_ms: timer.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    });

    protocol::write_message(connection.get_mut(), &response)
}

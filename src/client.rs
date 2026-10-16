use std::{
    io::{self, BufReader},
    os::unix::net::UnixStream,
    path::Path,
};

use crate::protocol::{self, Command, IO_TIMEOUT, Response, Status};

pub fn status(path: &Path) -> io::Result<Status> {
    let stream = UnixStream::connect(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "cannot connect to daemon at {}: {error}; start `stopwatch daemon` with the same socket path",
                path.display()
            ),
        )
    })?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    let mut connection = BufReader::new(stream);
    protocol::write_message(connection.get_mut(), &Command::Status)?;

    match protocol::read_message(&mut connection)? {
        Some(Response::Status(status)) => Ok(status),
        Some(Response::Error { error }) => Err(io::Error::other(format!("daemon: {error}"))),
        None => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "daemon disconnected without a response",
        )),
    }
}

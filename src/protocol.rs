use std::{
    io::{self, BufRead, Read, Write},
    time::Duration,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_MESSAGE_BYTES: u64 = 4096;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Status,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Paused,
    Running,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Status {
    pub state: State,
    pub elapsed_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Response {
    Status(Status),
    Error { error: String },
}

pub fn read_message<T: DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    reader.take(MAX_MESSAGE_BYTES).read_line(&mut line)?;

    if line.is_empty() {
        return Ok(None);
    }
    if !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message must end with a newline and fit within 4096 bytes",
        ));
    }

    serde_json::from_str(&line)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message exceeds 4096 bytes",
        ));
    }

    writer.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use super::*;

    #[test]
    fn reads_one_message_at_a_time_across_small_buffers() {
        let bytes = b"\"status\"\n\"status\"\n";
        let mut reader = BufReader::with_capacity(2, bytes.as_slice());

        for _ in 0..2 {
            assert!(matches!(
                read_message(&mut reader).unwrap(),
                Some(Command::Status)
            ));
        }
        assert!(read_message::<Command>(&mut reader).unwrap().is_none());
    }

    #[test]
    fn rejects_invalid_incomplete_and_oversized_messages() {
        let oversized = vec![b' '; MAX_MESSAGE_BYTES as usize];

        for mut bytes in [
            b"invalid\n".as_slice(),
            b"\"unknown\"\n",
            b"\"status\"",
            b"\xff\n",
            &oversized,
        ] {
            let error = read_message::<Command>(&mut bytes).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
    }
}

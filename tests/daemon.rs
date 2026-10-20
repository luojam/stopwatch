use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixStream,
    },
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct TestDaemon {
    directory: PathBuf,
    socket: PathBuf,
    child: Option<Child>,
}

impl TestDaemon {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "stopwatch-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self {
            socket: directory.join("stopwatch/stopwatch.sock"),
            directory,
            child: None,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_stopwatch"));
        command.env("XDG_RUNTIME_DIR", &self.directory);
        command
    }

    fn start(&mut self) {
        let stderr_path = self.directory.join("daemon.stderr");
        self.child = Some(
            self.command()
                .arg("daemon")
                .stdout(Stdio::null())
                .stderr(fs::File::create(&stderr_path).unwrap())
                .spawn()
                .unwrap(),
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                panic!(
                    "daemon exited during startup ({status}): {}",
                    fs::read_to_string(&stderr_path).unwrap()
                );
            }
            if UnixStream::connect(&self.socket).is_ok() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "daemon did not start: {}",
                fs::read_to_string(&stderr_path).unwrap()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn assert_paused_at_zero(&self) {
        let output = run(self.command().arg("status"));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"paused 0ms\n");
    }

    fn run_command(&self, action: &str) -> (String, u64) {
        let output = run(self.command().arg(action));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let (state, elapsed) = stdout.trim_end().split_once(' ').unwrap();
        let elapsed_ms = elapsed.strip_suffix("ms").unwrap().parse().unwrap();
        (state.to_owned(), elapsed_ms)
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn run(command: &mut Command) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);

    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "command {command:?} timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }

    child.wait_with_output().unwrap()
}

#[test]
fn new_daemon_reports_paused_at_zero() {
    let mut daemon = TestDaemon::new();
    daemon.start();
    daemon.assert_paused_at_zero();
}

#[test]
fn cli_controls_preserve_time_across_connections_and_reset_to_zero() {
    let mut daemon = TestDaemon::new();
    daemon.start();
    let (state, elapsed_ms) = daemon.run_command("pause");
    assert_eq!(state, "paused");
    assert_eq!(elapsed_ms, 0);

    let mut paused_elapsed = 0;
    for _ in 0..2 {
        let (state, elapsed_ms) = daemon.run_command("start");
        assert_eq!(state, "running");
        assert!(elapsed_ms >= paused_elapsed);

        let deadline = Instant::now() + Duration::from_secs(5);
        let running_elapsed = loop {
            let (state, elapsed) = daemon.run_command("status");
            assert_eq!(state, "running");
            if elapsed > paused_elapsed {
                break elapsed;
            }
            assert!(Instant::now() < deadline, "timer did not advance");
            thread::sleep(Duration::from_millis(10));
        };

        let (state, elapsed_ms) = daemon.run_command("start");
        assert_eq!(state, "running");
        assert!(elapsed_ms >= running_elapsed);

        let (state, elapsed_ms) = daemon.run_command("pause");
        assert_eq!(state, "paused");
        paused_elapsed = elapsed_ms;
        assert!(paused_elapsed >= running_elapsed);

        let (state, elapsed_ms) = daemon.run_command("status");
        assert_eq!(state, "paused");
        assert_eq!(elapsed_ms, paused_elapsed);

        let (state, elapsed_ms) = daemon.run_command("pause");
        assert_eq!(state, "paused");
        assert_eq!(elapsed_ms, paused_elapsed);
    }

    let (state, elapsed_ms) = daemon.run_command("reset");
    assert_eq!(state, "paused");
    assert_eq!(elapsed_ms, 0);
    daemon.assert_paused_at_zero();

    let (state, _) = daemon.run_command("start");
    assert_eq!(state, "running");

    let (state, elapsed_ms) = daemon.run_command("reset");
    assert_eq!(state, "paused");
    assert_eq!(elapsed_ms, 0);
    daemon.assert_paused_at_zero();

    let (state, _) = daemon.run_command("start");
    assert_eq!(state, "running");
}

#[test]
fn socket_and_directory_are_private() {
    let mut daemon = TestDaemon::new();
    daemon.start();

    assert_eq!(
        fs::metadata(daemon.socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&daemon.socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn rejects_a_second_daemon_without_disrupting_the_first() {
    let mut daemon = TestDaemon::new();
    daemon.start();

    let output = run(daemon.command().arg("daemon"));
    assert!(!output.status.success());
    daemon.assert_paused_at_zero();
}

#[test]
fn explicit_socket_path_works_without_runtime_directory() {
    let mut daemon = TestDaemon::new();
    daemon.start();

    let output = run(daemon
        .command()
        .env_remove("XDG_RUNTIME_DIR")
        .arg("--socket")
        .arg(&daemon.socket)
        .arg("status"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"paused 0ms\n");
}

#[test]
fn bad_requests_and_disconnects_do_not_stop_the_daemon() {
    let mut daemon = TestDaemon::new();
    daemon.start();

    let mut stream = UnixStream::connect(&daemon.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(b"invalid\n").unwrap();

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(response["error"].is_string());

    let mut remaining = String::new();
    assert_eq!(reader.read_line(&mut remaining).unwrap(), 0);
    daemon.assert_paused_at_zero();

    drop(UnixStream::connect(&daemon.socket).unwrap());
    daemon.assert_paused_at_zero();
}

#[test]
fn preserves_existing_files_and_symlinks() {
    let daemon = TestDaemon::new();
    fs::create_dir(daemon.socket.parent().unwrap()).unwrap();
    fs::write(&daemon.socket, "keep this file").unwrap();

    let output = run(daemon.command().arg("daemon"));
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(&daemon.socket).unwrap(),
        "keep this file"
    );
    fs::remove_file(&daemon.socket).unwrap();

    let target = daemon.directory.join("target");
    fs::write(&target, "keep this too").unwrap();
    symlink(&target, &daemon.socket).unwrap();
    let output = run(daemon.command().arg("daemon"));
    assert!(!output.status.success());
    assert_eq!(fs::read_link(&daemon.socket).unwrap(), target);
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep this too");
}

#[test]
fn reports_missing_daemon() {
    let daemon = TestDaemon::new();
    let output = run(daemon.command().arg("status"));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot connect to daemon"), "{stderr}");
}

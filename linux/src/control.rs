//! The running program listens on a unix socket so the command line (and the launcher's menu entries) can steer it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

pub fn socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir()) {
        return dir.join("snowdrift.sock");
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    PathBuf::from(format!("/tmp/snowdrift-{user}.sock"))
}

/// Sends one command to the running program and returns its answer; none if nothing runs.
pub fn send(command: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).ok()?;
    Some(answer.trim().to_string())
}

pub struct Server {
    listener: UnixListener,
}

impl Server {
    /// Starts listening, unless another copy already does.
    pub fn bind() -> Result<Server, String> {
        let path = socket_path();
        if send("ping").is_some() {
            return Err("Snowdrift is already running".into());
        }
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).map_err(|e| format!("can't listen on {}: {e}", path.display()))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Server { listener })
    }

    /// Every command waiting, each with the way to answer it.
    pub fn poll(&self) -> Vec<(String, UnixStream)> {
        let mut commands = Vec::new();
        while let Ok((stream, _)) = self.listener.accept() {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
            let mut line = String::new();
            if let Ok(clone) = stream.try_clone()
                && BufReader::new(clone).read_line(&mut line).is_ok()
            {
                commands.push((line.trim().to_string(), stream));
            }
        }
        commands
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(socket_path());
    }
}

pub fn answer(mut stream: UnixStream, text: &str) {
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.write_all(b"\n");
}

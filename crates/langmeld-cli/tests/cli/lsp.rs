use std::{
    fs,
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use serde_json::Value;

use super::{TestDirectory, write_widget_project};

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send_lsp_message(writer: &mut impl Write, message: Value) {
    let body = serde_json::to_vec(&message).expect("message should serialize");
    write!(writer, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    writer.write_all(&body).unwrap();
    writer.flush().unwrap();
}

fn read_lsp_message(reader: &mut impl BufRead) -> io::Result<Value> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "language server closed stdout",
            ));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((header, value)) = line.split_once(':')
            && header.eq_ignore_ascii_case("Content-Length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
    }

    let mut body = vec![
        0;
        content_length.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
        })?
    ];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn receive_lsp_message(receiver: &mpsc::Receiver<io::Result<Value>>) -> Value {
    receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("language server should respond before timeout")
        .expect("language server should send a valid LSP message")
}

fn canonical_file_uri(uri: &str) -> String {
    let path = url::Url::parse(uri)
        .expect("file URI should be valid")
        .to_file_path()
        .expect("URI should identify a local file");
    url::Url::from_file_path(fs::canonicalize(path).expect("file should exist"))
        .expect("canonical path should form a file URI")
        .to_string()
}

fn start_lsp(
    args: &[&str],
    current_directory: Option<&Path>,
) -> (
    ChildGuard,
    std::process::ChildStdin,
    mpsc::Receiver<io::Result<Value>>,
) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_langmeld"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(current_directory) = current_directory {
        command.current_dir(current_directory);
    }
    let mut child = ChildGuard(command.spawn().expect("CLI should start"));
    let stdin = child.0.stdin.take().unwrap();
    let stdout = child.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Ok(message) = read_lsp_message(&mut reader) {
            if sender.send(Ok(message)).is_err() {
                break;
            }
        }
    });
    (child, stdin, receiver)
}

#[path = "lsp/commands.rs"]
mod commands;
#[path = "lsp/project_definitions.rs"]
mod project_definitions;

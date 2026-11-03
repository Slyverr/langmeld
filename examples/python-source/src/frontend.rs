use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use langmeld::core::{Diagnostic, SourceFile, Span};
use serde_json::Value as JsonValue;

const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_STARTUP_BYTES: usize = 1024;

pub(super) struct PythonFrontend {
    executable: std::ffi::OsString,
    child: Option<Child>,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: Option<BufReader<ChildStdout>>,
}

impl PythonFrontend {
    pub(super) fn new() -> Self {
        Self {
            executable: std::env::var_os("LANGMELD_PYTHON").unwrap_or_else(|| "python3".into()),
            child: None,
            stdin: None,
            stdout: None,
        }
    }

    pub(super) fn parse(&mut self, source: &SourceFile) -> Result<JsonValue, Diagnostic> {
        if source.text.len() > MAX_SOURCE_BYTES {
            return Err(Diagnostic::error(
                "PS1003",
                format!("Python source exceeds the {MAX_SOURCE_BYTES} byte adapter limit"),
                Span::new(0, source.text.len()),
            ));
        }
        self.ensure_started()?;
        let request = serde_json::json!({ "source": source.text });
        let send_result = {
            let stdin = self.stdin.as_mut().expect("started frontend has stdin");
            serde_json::to_writer(&mut *stdin, &request)
                .and_then(|()| stdin.write_all(b"\n").map_err(serde_json::Error::io))
                .and_then(|()| stdin.flush().map_err(serde_json::Error::io))
        };
        if let Err(error) = send_result {
            return Err(
                self.worker_failure(&format!("cannot send request to Python frontend: {error}"))
            );
        }

        let mut line = Vec::new();
        let read_result = self
            .stdout
            .as_mut()
            .expect("started frontend has stdout")
            .take((MAX_RESPONSE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line);
        let read = match read_result {
            Ok(read) => read,
            Err(error) => {
                return Err(
                    self.worker_failure(&format!("cannot read Python frontend response: {error}"))
                );
            }
        };
        if read == 0 {
            return Err(self.worker_failure("Python frontend exited before returning a response"));
        }
        if line.len() > MAX_RESPONSE_BYTES || line.last() != Some(&b'\n') {
            self.stop_worker();
            return Err(Diagnostic::spec_error(
                "PS1000",
                format!(
                    "Python frontend response exceeds the {MAX_RESPONSE_BYTES} byte limit or is incomplete"
                ),
            ));
        }
        let response: JsonValue =
            serde_json::from_slice(&line[..line.len() - 1]).map_err(|error| {
                self.stop_worker();
                Diagnostic::spec_error(
                    "PS1002",
                    format!("Python frontend returned invalid JSON: {error}"),
                )
            })?;
        if let Some(error) = response.get("frontend_error").and_then(JsonValue::as_str) {
            return Err(Diagnostic::spec_error(
                "PS1002",
                format!("Python frontend rejected the request: {error}"),
            ));
        }
        Ok(response)
    }

    fn ensure_started(&mut self) -> Result<(), Diagnostic> {
        if self.child.is_some() {
            return Ok(());
        }
        let package_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new(&self.executable)
            .args(["-I", "-S", "-u", "indexer.py"])
            .current_dir(package_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                Diagnostic::spec_error(
                    "PS1000",
                    format!("cannot start CPython AST frontend: {error}"),
                )
            })?;
        let stdin = child.stdin.take().expect("piped child stdin");
        let stdout = child.stdout.take().expect("piped child stdout");
        self.stdin = Some(BufWriter::new(stdin));
        self.stdout = Some(BufReader::new(stdout));
        self.child = Some(child);

        let mut ready = Vec::new();
        let read = match self
            .stdout
            .as_mut()
            .expect("frontend stdout was set")
            .take(MAX_STARTUP_BYTES as u64)
            .read_until(b'\n', &mut ready)
        {
            Ok(read) => read,
            Err(error) => {
                self.stop_worker();
                return Err(Diagnostic::spec_error(
                    "PS1000",
                    format!("cannot initialize CPython AST frontend: {error}"),
                ));
            }
        };
        let response: JsonValue =
            match serde_json::from_slice(ready.strip_suffix(b"\n").unwrap_or(&ready)) {
                Ok(response) => response,
                Err(error) => {
                    self.stop_worker();
                    return Err(Diagnostic::spec_error(
                        "PS1000",
                        format!("CPython frontend startup handshake is invalid: {error}"),
                    ));
                }
            };
        if read == 0 || ready.len() > MAX_STARTUP_BYTES || response["ready"] != true {
            self.stop_worker();
            return Err(Diagnostic::spec_error(
                "PS1000",
                "CPython frontend did not become ready",
            ));
        }
        Ok(())
    }

    fn worker_failure(&mut self, message: &str) -> Diagnostic {
        let status = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten());
        self.stop_worker();
        Diagnostic::spec_error(
            "PS1000",
            status.map_or_else(
                || message.to_owned(),
                |status| format!("{message} (worker exited with {status})"),
            ),
        )
    }

    fn stop_worker(&mut self) {
        self.stdin.take();
        self.stdout.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

impl Drop for PythonFrontend {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

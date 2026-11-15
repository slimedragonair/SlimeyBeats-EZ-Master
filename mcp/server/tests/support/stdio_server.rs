//! Bounded real-process client for scratch lifecycle tests; owns every child it kills.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub struct Server {
    child: Child,
    replies: mpsc::Receiver<Result<Value, String>>,
    errors: Arc<Mutex<String>>,
    sequence: u64,
}

impl Server {
    pub fn start(input: &Path, output: &Path, desktop: Option<&Path>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_slimeybeats-mcp"));
        command
            .arg("--input-root")
            .arg(input)
            .arg("--output-root")
            .arg(output)
            .args(["--max-decoded-mib", "8", "--decode-timeout-seconds", "5"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(desktop) = desktop {
            command.arg("--desktop-exe").arg(desktop);
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let reply = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|error| error.to_string())
                });
                if send.send(reply).is_err() {
                    break;
                }
            }
        });
        let errors = Arc::new(Mutex::new(String::new()));
        let captured = errors.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                captured.lock().unwrap().push_str(&line.unwrap());
            }
        });
        let mut server = Self {
            child,
            replies,
            errors,
            sequence: 0,
        };
        server.request(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},
                "clientInfo":{"name":"scratch-lifecycle-test","version":"1"}}),
        );
        server.send(json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}));
        server
    }

    fn send(&mut self, value: Value) {
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.sequence += 1;
        self.send(json!({"jsonrpc":"2.0","id":self.sequence,"method":method,"params":params}));
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let reply = self
                .replies
                .recv_timeout(remaining)
                .unwrap_or_else(|error| {
                    panic!("{method}: {error}; {}", self.errors.lock().unwrap())
                })
                .unwrap();
            if reply["id"] == self.sequence {
                assert!(reply.get("error").is_none(), "{reply}");
                return reply["result"].clone();
            }
        }
    }

    pub fn call(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.request("tools/call", json!({"name":name,"arguments":arguments}));
        assert_ne!(result["isError"], true, "{result}");
        result["structuredContent"].clone()
    }

    pub fn run(&mut self, name: &str, arguments: Value) -> Value {
        let job = self.call(name, arguments);
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let status = self.call("get_job_status", json!({"job_id":job["job_id"]}));
            match status["status"].as_str().unwrap() {
                "completed" => return status["result"].clone(),
                "failed" | "cancelled" => panic!("{name}: {status}"),
                _ => assert!(Instant::now() < deadline, "{name}: {status}"),
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn close(&mut self) {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "{status}; {}",
                    self.errors.lock().unwrap()
                );
                return;
            }
            assert!(Instant::now() < deadline, "Companion did not exit on EOF");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn kill(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        drop(self.child.stdin.take());
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

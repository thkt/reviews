pub mod knip;
pub mod markuplint;
pub mod oxlint;
pub mod react_doctor;
pub mod tsgo;

use crate::sanitize;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const TOOL_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_OUTPUT_SIZE: usize = 102_400;
/// Total budget for combined additionalContext across all tools
const MAX_TOTAL_OUTPUT: usize = 204_800;

#[derive(Debug)]
pub struct ToolResult {
    pub name: &'static str,
    pub output: String,
    pub success: bool,
}

impl ToolResult {
    pub fn skipped(name: &'static str) -> Self {
        Self {
            name,
            output: String::new(),
            success: false,
        }
    }
}

fn combine_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut buf = String::with_capacity(stdout.len() + stderr.len() + 1);
    if !stdout.is_empty() {
        buf.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(&stderr);
    }

    let sanitized = sanitize::sanitize(&buf);
    truncate_output(&sanitized)
}

fn truncate_output(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_SIZE {
        s.to_string()
    } else {
        let mut truncated = s[..s.floor_char_boundary(MAX_OUTPUT_SIZE)].to_string();
        truncated.push_str("\n[output truncated]");
        truncated
    }
}

pub fn enforce_total_budget(results: &mut [ToolResult]) {
    let mut total = 0usize;
    for result in results.iter_mut() {
        total += result.output.len();
        if total > MAX_TOTAL_OUTPUT {
            result.output = "[omitted: total output budget exceeded]".into();
        }
    }
}

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

fn kill_process_group(pid: u32) {
    // Safety: kill(-pid) sends signal to the process group led by `pid`.
    unsafe {
        kill(-(pid as i32), 9);
    }
}

fn run_with_timeout_duration(
    name: &'static str,
    mut cmd: Command,
    timeout: Duration,
) -> ToolResult {
    cmd.process_group(0);

    let child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("reviews: {} spawn error: {}", name, e);
            return ToolResult::skipped(name);
        }
    };

    let pid = child.id();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => ToolResult {
            name,
            success: output.status.success(),
            output: combine_output(&output),
        },
        Ok(Err(e)) => {
            eprintln!("reviews: {} output read error: {}", name, e);
            ToolResult::skipped(name)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            eprintln!(
                "reviews: {} timed out after {}s, killing process group",
                name,
                timeout.as_secs()
            );
            kill_process_group(pid);
            ToolResult::skipped(name)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            eprintln!("reviews: {} wait thread disconnected", name);
            ToolResult::skipped(name)
        }
    }
}

fn run_with_timeout(name: &'static str, cmd: Command) -> ToolResult {
    run_with_timeout_duration(name, cmd, TOOL_TIMEOUT)
}

pub(crate) fn run_js_command(
    name: &'static str,
    bin: &Path,
    args: &[&str],
    info: &crate::project::ProjectInfo,
) -> ToolResult {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(&info.root);
    run_with_timeout(name, cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    #[test]
    fn skipped_result_is_empty_and_failed() {
        let r = ToolResult::skipped("test-tool");
        assert_eq!(r.name, "test-tool");
        assert!(!r.success);
        assert!(r.output.is_empty());
    }

    #[test]
    fn combine_output_stdout_only() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: b"hello world".to_vec(),
            stderr: vec![],
        };
        assert_eq!(combine_output(&output), "hello world");
    }

    #[test]
    fn combine_output_stderr_only() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: b"error msg".to_vec(),
        };
        assert_eq!(combine_output(&output), "error msg");
    }

    #[test]
    fn combine_output_both_streams() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        };
        assert_eq!(combine_output(&output), "out\nerr");
    }

    #[test]
    fn combine_output_truncates_large_output() {
        let big = "x".repeat(MAX_OUTPUT_SIZE + 1000);
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: big.into_bytes(),
            stderr: vec![],
        };
        let result = combine_output(&output);
        assert!(result.len() <= MAX_OUTPUT_SIZE + 50);
        assert!(result.ends_with("[output truncated]"));
    }

    #[test]
    fn run_with_timeout_success() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        let result = run_with_timeout("echo-test", cmd);
        assert!(result.success);
        assert!(result.output.contains("hello"));
    }

    #[test]
    fn run_with_timeout_handles_missing_command() {
        let cmd = Command::new("nonexistent-command-12345");
        let result = run_with_timeout("missing", cmd);
        assert!(!result.success);
        assert!(result.output.is_empty());
    }

    #[test]
    fn run_with_timeout_captures_exit_code() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo fail >&2; exit 1"]);
        let result = run_with_timeout("fail-test", cmd);
        assert!(!result.success);
        assert!(result.output.contains("fail"));
    }

    #[test]
    fn run_with_timeout_duration_kills_on_timeout() {
        let mut cmd = Command::new("sleep");
        cmd.arg("120");
        let result = run_with_timeout_duration("sleep-test", cmd, Duration::from_millis(200));
        assert!(!result.success);
        assert!(result.output.is_empty());
    }

    #[test]
    fn enforce_total_budget_truncates_excess() {
        let mut results = vec![
            ToolResult {
                name: "a",
                output: "x".repeat(MAX_TOTAL_OUTPUT),
                success: true,
            },
            ToolResult {
                name: "b",
                output: "overflow".into(),
                success: true,
            },
        ];
        enforce_total_budget(&mut results);
        assert_eq!(results[0].output.len(), MAX_TOTAL_OUTPUT);
        assert!(results[1].output.contains("budget exceeded"));
    }

    #[test]
    fn enforce_total_budget_no_truncation_when_within_limit() {
        let mut results = vec![
            ToolResult {
                name: "a",
                output: "small".into(),
                success: true,
            },
            ToolResult {
                name: "b",
                output: "also small".into(),
                success: true,
            },
        ];
        enforce_total_budget(&mut results);
        assert_eq!(results[0].output, "small");
        assert_eq!(results[1].output, "also small");
    }
}

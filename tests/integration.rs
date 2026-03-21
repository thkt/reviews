use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "reviews-integ-{}-{}-{}",
            prefix,
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_reviews_in(dir: &std::path::Path, input: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_reviews"))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn reviews");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    )
}

fn run_reviews(input: &str) -> (String, String, bool) {
    let tmp = TempDir::new("default");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    run_reviews_in(tmp.path(), input)
}

#[test]
fn non_target_skill_exits_silently() {
    let tmp = TempDir::new("nonmatch");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    std::fs::write(
        tmp.path().join(".claude/tools.json"),
        r#"{"reviews": {"skills": ["audit"]}}"#,
    )
    .unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "commit"}}"#;
    let (stdout, _, success) = run_reviews_in(tmp.path(), input);
    assert!(success);
    assert!(stdout.is_empty());
}

#[test]
fn no_config_warns_about_setup() {
    let tmp = TempDir::new("no-config");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
    let (_, stderr, success) = run_reviews_in(tmp.path(), input);
    assert!(success);
    assert!(
        stderr.contains("no config found"),
        "expected no-config hint, got: {stderr}"
    );
    assert!(
        !stderr.contains("running on all skills"),
        "should not print skills hint when no config, got: {stderr}"
    );
}

#[test]
fn tools_json_without_skills_warns_about_filtering() {
    let tmp = TempDir::new("no-skills");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    std::fs::write(
        tmp.path().join(".claude/tools.json"),
        r#"{"reviews": {"enabled": true}}"#,
    )
    .unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
    let (_, stderr, success) = run_reviews_in(tmp.path(), input);
    assert!(success);
    assert!(
        stderr.contains("running on all skills"),
        "expected skills hint, got: {stderr}"
    );
    assert!(
        !stderr.contains("no config found"),
        "should not print no-config hint when tools.json exists, got: {stderr}"
    );
}

#[test]
fn legacy_config_without_skills_warns_about_filtering() {
    let tmp = TempDir::new("legacy-no-skills");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::write(
        tmp.path().join(".claude-reviews.json"),
        r#"{"enabled": true}"#,
    )
    .unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
    let (_, stderr, success) = run_reviews_in(tmp.path(), input);
    assert!(success);
    assert!(
        stderr.contains("running on all skills"),
        "expected skills hint for legacy config, got: {stderr}"
    );
    assert!(
        !stderr.contains("no config found"),
        "should not print no-config hint for legacy config, got: {stderr}"
    );
}

#[test]
fn configured_skills_runs_without_warning() {
    let tmp = TempDir::new("no-hint");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    std::fs::write(
        tmp.path().join(".claude/tools.json"),
        r#"{"reviews": {"skills": ["audit"]}}"#,
    )
    .unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
    let (_, stderr, success) = run_reviews_in(tmp.path(), input);
    assert!(success);
    assert!(
        !stderr.contains("no config found"),
        "should not print no-config hint when configured, got: {stderr}"
    );
    assert!(
        !stderr.contains("running on all skills"),
        "should not print skills hint when configured, got: {stderr}"
    );
}

#[test]
fn invalid_json_exits_silently() {
    let (stdout, _, success) = run_reviews("not json{{{");
    assert!(success);
    assert!(stdout.is_empty());
}

#[test]
fn empty_input_exits_silently() {
    let (stdout, _, success) = run_reviews("");
    assert!(success);
    assert!(stdout.is_empty());
}

#[test]
fn disabled_config_exits_silently() {
    let tmp = TempDir::new("disabled");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::write(
        tmp.path().join(".claude-reviews.json"),
        r#"{"enabled": false}"#,
    )
    .unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "review"}}"#;
    let (stdout, _, success) = run_reviews_in(tmp.path(), input);
    assert!(success);
    assert!(stdout.is_empty());
}

#[test]
fn default_skill_does_not_crash() {
    let tmp = TempDir::new("review-nocrash");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "review"}}"#;
    let (stdout, _, success) = run_reviews_in(tmp.path(), input);
    assert!(success);

    if !stdout.is_empty() {
        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("invalid JSON output: {e}\nstdout: {stdout}"));
        assert_eq!(parsed["decision"], "approve");
    }
}

#[test]
fn configured_skill_does_not_crash() {
    let tmp = TempDir::new("configured-nocrash");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::write(
        tmp.path().join(".claude-reviews.json"),
        r#"{"skills": ["audit"]}"#,
    )
    .unwrap();

    let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
    let (stdout, _, success) = run_reviews_in(tmp.path(), input);
    assert!(success);

    if !stdout.is_empty() {
        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("invalid JSON output: {e}\nstdout: {stdout}"));
        assert_eq!(parsed["decision"], "approve");
    }
}

#[test]
fn exits_zero_on_valid_processing() {
    let (_, _, success) = run_reviews(r#"{"tool_name": "Skill", "tool_input": {}}"#);
    assert!(success, "should exit 0 even when skill field is missing");
}

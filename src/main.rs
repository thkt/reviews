mod config;
mod project;
mod resolve;
mod sanitize;
#[cfg(test)]
mod test_utils;
mod tools;
mod traverse;

use serde::Deserialize;
use std::io::{self, Read};
use std::path::Path;
use std::sync::LazyLock;

static DEBUG: LazyLock<bool> = LazyLock::new(|| std::env::var("REVIEWS_DEBUG").is_ok());

const MAX_INPUT_SIZE: usize = 10_000_000;

#[derive(Deserialize)]
struct HookInput {
    tool_input: SkillInput,
}

#[derive(Deserialize)]
struct SkillInput {
    skill: Option<String>,
}

fn parse_skill_name(input: &str) -> Option<String> {
    let hook: HookInput = serde_json::from_str(input).ok()?;
    hook.tool_input.skill
}

fn build_output(results: &[tools::ToolResult]) -> Option<String> {
    if results.is_empty() {
        return None;
    }

    let with_output: Vec<_> = results.iter().filter(|r| !r.output.is_empty()).collect();

    let mut context = String::new();
    for result in &with_output {
        context.push_str(&format!(
            "## {}\n\n``````\n{}\n``````\n\n",
            result.name, result.output
        ));
    }

    let with_issues = with_output.iter().filter(|r| !r.success).count();
    let summaries: Vec<String> = with_output
        .iter()
        .map(|r| {
            let lines = r.output.lines().count();
            let status = if r.success { "ok" } else { "issues" };
            format!("{}: {} lines ({})", r.name, lines, status)
        })
        .collect();
    let mut reason = format!(
        "Pre-flight: {}/{} tools reported",
        with_output.len(),
        results.len()
    );
    if with_issues > 0 {
        reason.push_str(&format!(" ({} with issues)", with_issues));
    }
    if !summaries.is_empty() {
        reason.push_str(" | ");
        reason.push_str(&summaries.join(", "));
    }
    let output = serde_json::json!({
        "decision": "approve",
        "reason": reason,
        "additionalContext": context.trim_end()
    });

    Some(output.to_string())
}

fn run(input: &str, cwd: &Path) -> Option<String> {
    let skill = parse_skill_name(input)?;
    let config = config::Config::load(cwd);

    if *DEBUG {
        eprintln!(
            "reviews: debug: skill={skill}, enabled={}, skills={:?}",
            config.enabled, config.skills
        );
    }

    if !config.enabled {
        return None;
    }

    if config.source == config::ConfigSource::Default {
        eprintln!(
            "Reviews: no config found. \
             Add .claude/tools.json: \
             {{\"reviews\":{{\"skills\":[\"{skill}\"]}}}} \
             — see https://github.com/thkt/reviews#configuration"
        );
    }

    if let Some(skills) = &config.skills {
        if !skills.contains(&skill) {
            if *DEBUG {
                eprintln!("reviews: debug: skill={skill} not in {skills:?}, skipping");
            }
            return None;
        }
    } else if config.source != config::ConfigSource::Default {
        eprintln!(
            "Reviews: running on all skills. \
             Filter via .claude/tools.json: \
             {{\"reviews\":{{\"skills\":[\"{skill}\"]}}}} \
             — see https://github.com/thkt/reviews#configuration"
        );
    }

    let project = project::ProjectInfo::detect(cwd);

    if *DEBUG {
        eprintln!(
            "reviews: debug: root={}, pkg={}, ts={}, react={}",
            project.root.display(),
            project.has_package_json,
            project.has_tsconfig,
            project.has_react
        );
    }

    let start = std::time::Instant::now();
    let mut results = run_tools_parallel(&config, &project);
    tools::enforce_total_budget(&mut results);
    warn_missing_tools(&results);

    if *DEBUG {
        eprintln!(
            "reviews: debug: completed in {}ms",
            start.elapsed().as_millis()
        );
    }

    build_output(&results)
}

fn warn_missing_tools(results: &[tools::ToolResult]) {
    for result in results {
        if !result.output.is_empty() || result.success {
            continue;
        }
        if let Some(info) = tools::INSTALL_COMMANDS
            .iter()
            .find(|i| i.name == result.name)
        {
            eprintln!(
                "Reviews: {} not installed. Install: {}",
                result.name, info.install
            );
        } else {
            eprintln!("Reviews: {} not installed. Install manually.", result.name);
        }
    }
}

fn main() {
    let mut input_str = String::new();
    let bytes_read = match io::stdin()
        .take((MAX_INPUT_SIZE + 1) as u64)
        .read_to_string(&mut input_str)
    {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Reviews: stdin read error: {}", e);
            return;
        }
    };

    if bytes_read > MAX_INPUT_SIZE {
        eprintln!(
            "Reviews: warning: input too large (>{}B limit), skipping",
            MAX_INPUT_SIZE
        );
        return;
    }

    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Reviews: cannot determine cwd: {}", e);
            return;
        }
    };

    if let Some(json) = run(&input_str, &cwd) {
        println!("{}", json);
    }
}

fn run_tools_parallel(
    config: &config::Config,
    project: &project::ProjectInfo,
) -> Vec<tools::ToolResult> {
    use std::thread;

    type ToolRunFn = fn(&project::ProjectInfo) -> tools::ToolResult;

    struct Entry {
        enabled: bool,
        name: &'static str,
        run: ToolRunFn,
    }

    let entries = vec![
        Entry {
            enabled: config.tools.knip,
            name: "knip",
            run: tools::knip::run,
        },
        Entry {
            enabled: config.tools.oxlint,
            name: "oxlint",
            run: tools::oxlint::run,
        },
        Entry {
            enabled: config.tools.tsgo,
            name: "tsgo",
            run: tools::tsgo::run,
        },
        Entry {
            enabled: config.tools.react_doctor,
            name: "react-doctor",
            run: tools::react_doctor::run,
        },
        Entry {
            enabled: config.tools.markuplint,
            name: "markuplint",
            run: tools::markuplint::run,
        },
    ];

    let handles: Vec<_> = entries
        .into_iter()
        .filter(|e| e.enabled)
        .map(|e| {
            let p = project.clone();
            (e.name, thread::spawn(move || (e.run)(&p)))
        })
        .collect();

    handles
        .into_iter()
        .map(|(name, handle)| match handle.join() {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Reviews: {} thread panicked: {:?}", name, e);
                tools::ToolResult::skipped(name)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_name_valid() {
        let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
        assert_eq!(parse_skill_name(input).as_deref(), Some("audit"));
    }

    #[test]
    fn parse_skill_name_invalid_json() {
        assert!(parse_skill_name("not json{{{").is_none());
    }

    #[test]
    fn parse_skill_name_missing() {
        let input = r#"{"tool_name": "Skill", "tool_input": {}}"#;
        assert!(parse_skill_name(input).is_none());
    }

    #[test]
    fn parse_skill_name_with_args() {
        let input =
            r#"{"tool_name": "Skill", "tool_input": {"skill": "audit", "args": "--verbose"}}"#;
        assert_eq!(parse_skill_name(input).as_deref(), Some("audit"));
    }

    #[test]
    fn build_output_partial_success() {
        let results = vec![
            tools::ToolResult {
                name: "knip",
                output: "result1".into(),
                success: true,
            },
            tools::ToolResult {
                name: "oxlint",
                output: "result2".into(),
                success: true,
            },
            tools::ToolResult {
                name: "tsgo",
                output: "result3".into(),
                success: true,
            },
            tools::ToolResult {
                name: "react-doctor",
                output: String::new(),
                success: false,
            },
        ];
        let json = build_output(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["decision"], "approve");
        assert!(parsed["reason"].as_str().unwrap().contains("3/4"));
        let ctx = parsed["additionalContext"].as_str().unwrap();
        assert!(ctx.contains("knip"));
        assert!(ctx.contains("oxlint"));
        assert!(ctx.contains("tsgo"));
    }

    #[test]
    fn build_output_all_empty_output() {
        let results = vec![
            tools::ToolResult {
                name: "knip",
                output: String::new(),
                success: false,
            },
            tools::ToolResult {
                name: "oxlint",
                output: String::new(),
                success: false,
            },
        ];
        let json = build_output(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["decision"], "approve");
        assert!(parsed["reason"].as_str().unwrap().contains("0/2"));
    }

    #[test]
    fn build_output_empty_slice() {
        assert!(build_output(&[]).is_none());
    }

    #[test]
    fn build_output_includes_failed_with_output() {
        let results = vec![
            tools::ToolResult {
                name: "oxlint",
                output: "warning: unused variable".into(),
                success: false,
            },
            tools::ToolResult {
                name: "knip",
                output: String::new(),
                success: false,
            },
        ];
        let json = build_output(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let reason = parsed["reason"].as_str().unwrap();
        assert!(reason.contains("1/2"));
        assert!(reason.contains("1 with issues"));
        let ctx = parsed["additionalContext"].as_str().unwrap();
        assert!(ctx.contains("oxlint"));
        assert!(ctx.contains("warning: unused variable"));
    }

    #[test]
    fn build_output_excludes_successful_but_empty() {
        let results = vec![
            tools::ToolResult {
                name: "knip",
                output: String::new(),
                success: true,
            },
            tools::ToolResult {
                name: "oxlint",
                output: "issues".into(),
                success: true,
            },
        ];
        let json = build_output(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["reason"].as_str().unwrap().contains("1/2"));
        let ctx = parsed["additionalContext"].as_str().unwrap();
        assert!(!ctx.contains("knip"));
    }

    #[test]
    fn run_returns_none_for_non_matching_skill() {
        let tmp = test_utils::TempDir::new("run-nonmatch");
        std::fs::create_dir_all(tmp.join(".git")).unwrap();
        std::fs::create_dir_all(tmp.join(".claude")).unwrap();
        std::fs::write(
            tmp.join(".claude/tools.json"),
            r#"{"reviews": {"skills": ["audit"]}}"#,
        )
        .unwrap();
        let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "commit"}}"#;
        assert!(run(input, &tmp).is_none());
    }

    #[test]
    fn run_returns_none_when_disabled() {
        let tmp = test_utils::TempDir::new("run-disabled");
        std::fs::create_dir_all(tmp.join(".git")).unwrap();
        std::fs::write(tmp.join(".claude-reviews.json"), r#"{"enabled": false}"#).unwrap();
        let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "review"}}"#;
        assert!(run(input, &tmp).is_none());
    }

    #[test]
    fn run_returns_none_for_invalid_input() {
        let tmp = test_utils::TempDir::new("run-invalid");
        std::fs::create_dir_all(tmp.join(".git")).unwrap();
        assert!(run("not json", &tmp).is_none());
    }

    #[test]
    fn run_returns_none_for_skill_not_in_config() {
        let tmp = test_utils::TempDir::new("run-notinlist");
        std::fs::create_dir_all(tmp.join(".git")).unwrap();
        std::fs::write(tmp.join(".claude-reviews.json"), r#"{"skills": ["audit"]}"#).unwrap();
        let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "review"}}"#;
        assert!(run(input, &tmp).is_none());
    }

    #[test]
    fn run_returns_none_for_empty_skills() {
        let tmp = test_utils::TempDir::new("run-empty-skills");
        std::fs::create_dir_all(tmp.join(".git")).unwrap();
        std::fs::create_dir_all(tmp.join(".claude")).unwrap();
        std::fs::write(
            tmp.join(".claude/tools.json"),
            r#"{"reviews": {"skills": []}}"#,
        )
        .unwrap();
        let input = r#"{"tool_name": "Skill", "tool_input": {"skill": "audit"}}"#;
        assert!(run(input, &tmp).is_none());
    }

    #[test]
    fn warn_missing_tools_skipped_result() {
        let results = vec![tools::ToolResult {
            name: "knip",
            output: String::new(),
            success: false,
        }];
        // Should not panic; output goes to stderr
        warn_missing_tools(&results);
    }

    #[test]
    fn warn_missing_tools_ignores_successful() {
        let results = vec![tools::ToolResult {
            name: "knip",
            output: "some output".into(),
            success: true,
        }];
        // Successful results should not trigger warnings
        warn_missing_tools(&results);
    }

    #[test]
    fn warn_missing_tools_ignores_failed_with_output() {
        let results = vec![tools::ToolResult {
            name: "oxlint",
            output: "lint errors found".into(),
            success: false,
        }];
        // Failed with output = tool ran but found issues, not missing
        warn_missing_tools(&results);
    }

    #[test]
    fn thread_panic_returns_skipped() {
        use std::thread;

        let handle = thread::spawn(|| -> tools::ToolResult {
            panic!("simulated tool panic");
        });

        let result = match handle.join() {
            Ok(r) => r,
            Err(_) => tools::ToolResult::skipped("panicked-tool"),
        };

        assert!(!result.success);
        assert!(result.output.is_empty());
        assert_eq!(result.name, "panicked-tool");
    }
}

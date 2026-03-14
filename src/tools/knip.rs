use super::ToolResult;
use crate::project::ProjectInfo;
use crate::resolve;

pub fn run(project: &ProjectInfo) -> ToolResult {
    if !project.has_package_json {
        return ToolResult::skipped("knip");
    }

    let bin = resolve::resolve_bin("knip", &project.root);
    super::run_js_command(
        "knip",
        &bin,
        &["--reporter", "compact", "--no-exit-code"],
        project,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn skips_without_package_json() {
        let info = ProjectInfo {
            root: PathBuf::from("/tmp/nonexistent"),
            has_package_json: false,
            has_tsconfig: false,
            has_react: false,
        };
        let result = run(&info);
        assert!(!result.success);
        assert!(result.output.is_empty());
    }
}

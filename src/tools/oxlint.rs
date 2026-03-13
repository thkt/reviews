use super::ToolResult;
use crate::project::ProjectInfo;
use crate::resolve;

pub fn run(project: &ProjectInfo) -> ToolResult {
    if !project.has_package_json {
        return ToolResult::skipped("oxlint");
    }

    let bin = resolve::resolve_bin("oxlint", &project.root);
    super::run_js_command(
        "oxlint",
        &bin,
        &["--format", "json", "--ignore-pattern", "node_modules"],
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

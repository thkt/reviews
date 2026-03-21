use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub has_package_json: bool,
    pub has_tsconfig: bool,
    pub has_react: bool,
}

impl ProjectInfo {
    pub fn detect(dir: &Path) -> Self {
        let root = Self::find_root(dir);
        let has_tsconfig = root.join("tsconfig.json").exists();
        let pkg_json = Self::read_package_json(&root);
        let has_package_json = pkg_json.is_some();
        let has_react = pkg_json.as_ref().is_some_and(Self::has_react_dep);

        Self {
            root,
            has_package_json,
            has_tsconfig,
            has_react,
        }
    }

    fn find_root(start: &Path) -> PathBuf {
        crate::traverse::walk_ancestors(start, |dir| {
            dir.join(".git").exists().then(|| dir.to_path_buf())
        })
        .unwrap_or_else(|| start.to_path_buf())
    }

    fn read_package_json(root: &Path) -> Option<serde_json::Value> {
        let pkg_path = root.join("package.json");
        let content = match std::fs::read_to_string(&pkg_path) {
            Ok(c) => c,
            Err(_) => return None,
        };
        match serde_json::from_str(&content) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("Reviews: warning: invalid package.json: {}", e);
                None
            }
        }
    }

    fn has_react_dep(json: &serde_json::Value) -> bool {
        for key in ["dependencies", "devDependencies", "peerDependencies"] {
            if let Some(deps) = json.get(key).and_then(|v| v.as_object())
                && deps.contains_key("react")
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TempDir;
    use std::fs;

    #[test]
    fn detects_package_json() {
        let tmp = TempDir::new("project-pkg");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(tmp.join("package.json"), "{}").unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(info.has_package_json);
    }

    #[test]
    fn no_package_json() {
        let tmp = TempDir::new("project-nopkg");
        fs::create_dir_all(tmp.join(".git")).unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(!info.has_package_json);
    }

    #[test]
    fn detects_react_dependency() {
        let tmp = TempDir::new("project-react");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(
            tmp.join("package.json"),
            r#"{"dependencies": {"react": "^19.0.0"}}"#,
        )
        .unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(info.has_react);
    }

    #[test]
    fn detects_react_in_dev_dependencies() {
        let tmp = TempDir::new("project-react-dev");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(
            tmp.join("package.json"),
            r#"{"devDependencies": {"react": "^19.0.0"}}"#,
        )
        .unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(info.has_react);
    }

    #[test]
    fn no_react_dependency() {
        let tmp = TempDir::new("project-noreact");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(
            tmp.join("package.json"),
            r#"{"dependencies": {"vue": "^3.0.0"}}"#,
        )
        .unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(!info.has_react);
    }

    #[test]
    fn detects_react_in_peer_dependencies() {
        let tmp = TempDir::new("project-react-peer");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(
            tmp.join("package.json"),
            r#"{"peerDependencies": {"react": ">=18"}}"#,
        )
        .unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(info.has_react);
    }

    #[test]
    fn malformed_package_json_no_react() {
        let tmp = TempDir::new("project-malformed-pkg");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(tmp.join("package.json"), "not valid json").unwrap();

        let info = ProjectInfo::detect(&tmp);
        assert!(!info.has_react);
    }
}

use crate::traverse;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub fn resolve_bin(name: &str, start: &Path) -> PathBuf {
    debug_assert!(
        !name.contains('/') && !name.contains('\\') && !name.contains(".."),
        "binary name must not contain path components: {name}"
    );
    traverse::walk_ancestors(start, |dir| {
        let candidate = dir.join("node_modules/.bin").join(name);
        if candidate.exists() && is_executable(&candidate) {
            eprintln!("Reviews: resolved {} -> {}", name, candidate.display());
            Some(candidate)
        } else {
            None
        }
    })
    .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TempDir;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn finds_bin_in_node_modules() {
        let tmp = TempDir::new("resolve-find");
        let bin_dir = tmp.join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("knip");
        fs::write(&bin_path, "").unwrap();
        fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755)).unwrap();

        let result = resolve_bin("knip", &tmp);
        assert_eq!(result, bin_path);
    }

    #[test]
    fn skips_non_executable_bin() {
        let tmp = TempDir::new("resolve-noexec");
        let bin_dir = tmp.join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("knip");
        fs::write(&bin_path, "").unwrap();
        fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o644)).unwrap();

        let result = resolve_bin("knip", &tmp);
        assert_eq!(result, PathBuf::from("knip"));
    }

    #[test]
    fn falls_back_to_bare_name_when_no_node_modules() {
        let tmp = TempDir::new("resolve-nomod");
        fs::create_dir_all(tmp.join(".git")).unwrap();

        let result = resolve_bin("knip", &tmp);
        assert_eq!(result, PathBuf::from("knip"));
    }

    #[test]
    fn stops_at_git_boundary() {
        let tmp = TempDir::new("resolve-git");
        let project = tmp.join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        let bin_dir = tmp.join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin_path = bin_dir.join("knip");
        fs::write(&bin_path, "").unwrap();
        fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755)).unwrap();
        let subdir = project.join("src");
        fs::create_dir_all(&subdir).unwrap();

        let result = resolve_bin("knip", &subdir);
        assert_eq!(result, PathBuf::from("knip"));
    }
}

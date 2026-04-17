use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static P4_ROOT_CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn get_p4_root() -> Option<PathBuf> {
    P4_ROOT_CACHE.get_or_init(|| {
        if let Ok(cwd) = std::env::current_dir() {
            for ancestor in cwd.ancestors() {
                if ancestor.join(".p4config").is_file() {
                    return Some(ancestor.to_path_buf());
                }
            }
        }
        if let Ok(output) = std::process::Command::new("p4").arg("info").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Some(root) = line.strip_prefix("Client root: ") {
                        let p = PathBuf::from(root.trim());
                        if p.is_dir() { return Some(p); }
                    }
                }
            }
        }
        None
    }).clone()
}

pub fn get_project_root(data_dir: &Path) -> PathBuf {
    for ancestor in data_dir.ancestors() {
        if ancestor.join(".p4config").is_file() {
            return ancestor.to_path_buf();
        }
    }
    for ancestor in data_dir.ancestors() {
        if ancestor.join("MetaData").join("Schema").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    let fallback = data_dir.parent().and_then(|p| p.parent())
        .unwrap_or(data_dir).to_path_buf();
    if fallback.join("MetaData").is_dir() { return fallback; }
    if let Some(p4_root) = get_p4_root() { return p4_root; }
    fallback
}

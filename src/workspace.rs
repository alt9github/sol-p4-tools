use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use crate::p4::p4_bare;

static P4_ROOT_CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Find a directory whose ancestors contain a `.p4config` marker.
/// Returns the ancestor directory (the root of a branch workspace) if found.
pub fn find_p4config_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(".p4config").is_file() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// Directory containing the running executable. Stable across launch contexts
/// (shortcut, Explorer double-click, terminal), so this is the authoritative
/// starting point for "which branch am I running under?".
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// Walk up from `start` to find the nearest branch root.
/// Order: `.p4config` (authoritative per-branch marker) → `MetaData/Schema` dir.
pub fn find_branch_root(start: &Path) -> Option<PathBuf> {
    if let Some(p) = find_p4config_root(start) { return Some(p); }
    for ancestor in start.ancestors() {
        if ancestor.join("MetaData").join("Schema").is_dir() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// Cached Perforce workspace root. Resolution order:
///   1. exe dir walk → `.p4config` ancestor (most reliable on Windows shortcuts)
///   2. cwd walk → `.p4config` ancestor (dev convenience: `cargo run` in repo)
///   3. `p4 info` → "Client root:" (last resort — may be wrong branch if user
///      has a shared P4CLIENT env but multiple branches on disk)
pub fn get_p4_root() -> Option<PathBuf> {
    P4_ROOT_CACHE.get_or_init(|| {
        if let Some(exe) = exe_dir() {
            if let Some(root) = find_p4config_root(&exe) { return Some(root); }
        }
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(root) = find_p4config_root(&cwd) { return Some(root); }
        }
        if let Ok(output) = p4_bare().arg("info").output() {
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

/// Get project root from `data_dir` (a Deploy/GeneratedData_Server path or similar).
/// Strategies in order:
///   1. `.p4config` ancestor of data_dir (authoritative per-branch marker)
///   2. `MetaData/Schema` ancestor of data_dir
///   3. data_dir parent^2 fallback (data_dir assumed to be Deploy/GeneratedData_Server)
///   4. Cached p4 root (last resort — may point at a different branch)
pub fn get_project_root(data_dir: &Path) -> PathBuf {
    if let Some(root) = find_branch_root(data_dir) { return root; }
    let fallback = data_dir.parent().and_then(|p| p.parent())
        .unwrap_or(data_dir).to_path_buf();
    if fallback.join("MetaData").is_dir() { return fallback; }
    if let Some(p4_root) = get_p4_root() { return p4_root; }
    fallback
}

fn is_valid_data_dir(p: &Path) -> bool {
    p.is_dir() && p.join("UE").join("Level").is_dir()
}

fn find_data_dir_from(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let deploy = ancestor.join("Deploy").join("GeneratedData_Server");
        if is_valid_data_dir(&deploy) { return Some(deploy); }
        let direct = ancestor.join("GeneratedData_Server");
        if is_valid_data_dir(&direct) { return Some(direct); }
    }
    None
}

/// Locate the active branch's Deploy/GeneratedData_Server directory.
/// Order matches `get_p4_root` rationale: exe-relative first so Windows shortcut
/// launches pin to the correct branch, cwd only as a dev fallback.
#[tauri::command]
pub fn detect_data_dir() -> Option<String> {
    if let Some(exe) = exe_dir() {
        if let Some(found) = find_data_dir_from(&exe) {
            return Some(found.to_string_lossy().to_string());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(found) = find_data_dir_from(&cwd) {
            return Some(found.to_string_lossy().to_string());
        }
    }
    if let Some(p4_root) = get_p4_root() {
        let deploy = p4_root.join("Deploy").join("GeneratedData_Server");
        if is_valid_data_dir(&deploy) {
            return Some(deploy.to_string_lossy().to_string());
        }
        let direct = p4_root.join("GeneratedData_Server");
        if is_valid_data_dir(&direct) {
            return Some(direct.to_string_lossy().to_string());
        }
    }
    None
}

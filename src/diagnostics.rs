use std::path::{Path, PathBuf};
use crate::p4::{p4_bare, p4_cmd};

#[derive(serde::Serialize)]
pub struct Candidate {
    pub source: String,      // "exe_dir" | "cwd"
    pub path: Option<String>,
    pub p4config_ancestor: Option<String>,
    pub schema_ancestor: Option<String>,
    pub deploy_ancestor: Option<String>,
}

#[derive(serde::Serialize)]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub exe_path: Option<String>,
    pub cwd: Option<String>,
    pub p4_version: String,
    pub p4_info: String,
    pub p4_client_name: Option<String>,
    pub p4_client_root: Option<String>,
    pub p4_client_stream: Option<String>,
    pub chosen_project_root: Option<String>,
    pub chosen_data_dir: Option<String>,
    /// True when the chosen project root sits inside the active p4 client's
    /// Client root. False means the user's default client doesn't map the
    /// branch the app is running from — p4 ops will target a different
    /// directory than what the UI edits. Consumers use this to force the
    /// workspace selection prompt.
    pub client_matches_exe: bool,
    pub candidates: Vec<Candidate>,
}

fn to_str(p: &Path) -> String { p.to_string_lossy().to_string() }

fn find_p4config(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|a| a.join(".p4config").is_file()).map(|p| p.to_path_buf())
}

fn find_schema_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|a| a.join("MetaData").join("Schema").is_dir()).map(|p| p.to_path_buf())
}

fn find_deploy(start: &Path) -> Option<PathBuf> {
    for a in start.ancestors() {
        let d = a.join("Deploy").join("GeneratedData_Server");
        if d.join("UE").join("Level").is_dir() { return Some(d); }
        let d = a.join("GeneratedData_Server");
        if d.join("UE").join("Level").is_dir() { return Some(d); }
    }
    None
}

fn candidate(source: &str, path: Option<PathBuf>) -> Candidate {
    let p = path.as_ref();
    Candidate {
        source: source.to_string(),
        path: p.map(|p| to_str(p)),
        p4config_ancestor: p.and_then(|p| find_p4config(p)).map(|p| to_str(&p)),
        schema_ancestor: p.and_then(|p| find_schema_root(p)).map(|p| to_str(&p)),
        deploy_ancestor: p.and_then(|p| find_deploy(p)).map(|p| to_str(&p)),
    }
}

fn extract_info_field<'a>(p4_info: &'a str, prefix: &str) -> Option<&'a str> {
    p4_info.lines().find_map(|l| l.strip_prefix(prefix).map(|v| v.trim()))
}

/// Component-wise prefix check, case-insensitive on Windows. Avoids the
/// "C:\Perforce\foo" vs "C:\Perforce\foo_other" string-prefix false-match
/// where the directory names merely share a common stem.
fn is_path_prefix(ancestor: &Path, descendant: &Path) -> bool {
    let cmp = |a: &std::ffi::OsStr, b: &std::ffi::OsStr| -> bool {
        if cfg!(windows) {
            a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
        } else {
            a == b
        }
    };
    let mut a = ancestor.components();
    let mut d = descendant.components();
    loop {
        match (a.next(), d.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(x), Some(y)) if cmp(x.as_os_str(), y.as_os_str()) => continue,
            _ => return false,
        }
    }
}

/// Return structured info about how the app resolved its branch/data dir.
/// Frontend surfaces this in Settings → Diagnostics so users can see *why*
/// a wrong branch was picked (exe path vs cwd divergence, missing .p4config,
/// or p4 client pointing at a different branch on disk).
///
/// `app_version` is passed in from the host crate because this crate can't
/// know the consumer's package version at compile time.
pub fn collect(app_version: &str) -> Diagnostics {
    let exe_path = std::env::current_exe().ok();
    let exe_dir = exe_path.as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let cwd = std::env::current_dir().ok();

    let p4_version = p4_bare().arg("-V").output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
        .unwrap_or_else(|| "(p4 unavailable)".to_string());

    // Use p4_cmd so active override is reflected — this way, after the user
    // picks a workspace in the prompt, re-running diagnose shows the new state.
    let p4_info = p4_cmd().arg("info").output()
        .ok()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            if s.trim().is_empty() { String::from_utf8_lossy(&o.stderr).to_string() } else { s }
        })
        .unwrap_or_else(|| "(p4 unavailable)".to_string());

    let p4_client_name = extract_info_field(&p4_info, "Client name:").map(|s| s.to_string());
    let p4_client_root = extract_info_field(&p4_info, "Client root:").map(|s| s.to_string());
    let p4_client_stream = extract_info_field(&p4_info, "Client stream:").map(|s| s.to_string());

    let chosen_data_dir = crate::workspace::detect_data_dir();
    let chosen_project_root = chosen_data_dir.as_ref()
        .map(|d| crate::workspace::get_project_root(Path::new(d)))
        .map(|p| to_str(&p));

    // Mismatch = detected project root is not contained in (or equal to) p4 Client root.
    // Either direction counts as "same workspace" — client root could equal or contain
    // the project root depending on how the workspace was set up.
    let client_matches_exe = match (&chosen_project_root, &p4_client_root) {
        (Some(proj), Some(root)) => {
            let p = Path::new(proj);
            let r = Path::new(root);
            is_path_prefix(r, p) || is_path_prefix(p, r)
        }
        _ => false,
    };

    let candidates = vec![
        candidate("exe_dir", exe_dir),
        candidate("cwd", cwd.clone()),
    ];

    Diagnostics {
        app_version: app_version.to_string(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        exe_path: exe_path.as_ref().map(|p| to_str(p)),
        cwd: cwd.as_ref().map(|p| to_str(p)),
        p4_version: p4_version.trim().to_string(),
        p4_info: p4_info.trim().to_string(),
        p4_client_name,
        p4_client_root,
        p4_client_stream,
        chosen_project_root,
        chosen_data_dir,
        client_matches_exe,
        candidates,
    }
}

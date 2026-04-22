use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Default, Clone)]
pub struct P4Override {
    pub server: String,
    pub user: String,
    pub client: String,
}

static P4_OVERRIDE: OnceLock<Mutex<Option<P4Override>>> = OnceLock::new();

fn get_override() -> Option<P4Override> {
    P4_OVERRIDE.get_or_init(|| Mutex::new(None)).lock().ok()?.clone()
}

fn set_override(v: Option<P4Override>) {
    if let Ok(mut g) = P4_OVERRIDE.get_or_init(|| Mutex::new(None)).lock() {
        *g = v;
    }
}

/// Build a bare `p4` Command. On Windows, suppresses the console window that
/// would otherwise flash on every invocation from a GUI parent (Tauri webview).
/// Always route p4 spawns through this (or `p4_cmd` for the override-applied
/// variant) — do not call `Command::new("p4")` directly from consumers.
pub fn p4_bare() -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new("p4");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
        cmd.creation_flags(0x08000000);
    }
    cmd
}

pub fn p4_cmd() -> std::process::Command {
    let mut cmd = p4_bare();
    if let Some(o) = get_override() {
        if !o.server.is_empty() { cmd.args(["-p", &o.server]); }
        if !o.user.is_empty() { cmd.args(["-u", &o.user]); }
        if !o.client.is_empty() { cmd.args(["-c", &o.client]); }
    }
    cmd
}

#[derive(serde::Serialize, Clone)]
pub struct P4Workspace {
    pub name: String,
    pub stream: String,
    pub root: String,
}

#[tauri::command]
pub fn get_p4_stream(data_dir: Option<String>) -> String {
    let path = data_dir.as_deref().map(PathBuf::from);
    let config_dir = path.as_ref().and_then(|p| {
        p.ancestors().find(|a| a.join(".p4config").is_file()).map(|p| p.to_path_buf())
    });

    if let Some(o) = get_override() {
        if !o.client.is_empty() {
            let mut cmd = p4_bare();
            if !o.server.is_empty() { cmd.args(["-p", &o.server]); }
            if !o.user.is_empty() { cmd.args(["-u", &o.user]); }
            cmd.args(["client", "-o", &o.client]);
            if let Ok(out) = cmd.output() {
                if let Some(s) = parse_stream_from_spec(&String::from_utf8_lossy(&out.stdout)) {
                    return s;
                }
            }
        }
    }

    let run_p4 = |args: &[&str]| -> Option<String> {
        let mut cmd = p4_cmd();
        cmd.args(args);
        if let Some(ref d) = config_dir {
            cmd.current_dir(d);
            cmd.env("P4CONFIG", ".p4config");
        } else if let Some(ref p) = path {
            cmd.current_dir(p);
        }
        let output = cmd.output().ok()?;
        if !output.status.success() { return None; }
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    };

    if let Some(ref d) = config_dir {
        if let Ok(content) = std::fs::read_to_string(d.join(".p4config")) {
            let client = content.lines()
                .find_map(|l| l.strip_prefix("P4CLIENT=").map(|v| v.trim().to_string()));
            if let Some(c) = client {
                if let Some(out) = run_p4(&["client", "-o", &c]) {
                    if let Some(s) = parse_stream_from_spec(&out) { return s; }
                }
            }
        }
    }

    if let Some(out) = run_p4(&["switch"]) {
        for line in out.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with("//") { return t.to_lowercase(); }
        }
    }

    if let Some(out) = run_p4(&["info"]) {
        for line in out.lines() {
            if let Some(v) = line.strip_prefix("Client stream:") {
                if let Some(s) = extract_stream_name(v) { return s; }
            }
        }
    }

    String::new()
}

fn parse_stream_from_spec(spec: &str) -> Option<String> {
    for line in spec.lines() {
        if let Some(v) = line.strip_prefix("Stream:") {
            return extract_stream_name(v);
        }
    }
    None
}

fn extract_stream_name(raw: &str) -> Option<String> {
    let s = raw.trim().trim_start_matches("//");
    s.rsplit('/').next().filter(|n| !n.is_empty()).map(|n| n.to_lowercase())
}

#[tauri::command]
pub fn list_p4_workspaces(server: String, user: String) -> Result<Vec<P4Workspace>, String> {
    let mut cmd = p4_bare();
    if !server.is_empty() { cmd.args(["-p", &server]); }
    if !user.is_empty() { cmd.args(["-u", &user]); }
    cmd.args(["clients", "-u", &user]);

    let output = cmd.output().map_err(|e| format!("p4 unavailable: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(if err.is_empty() { "p4 clients failed".into() } else { err });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut workspaces = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || parts[0] != "Client" { continue; }
        let name = parts[1].to_string();
        let mut spec_cmd = p4_bare();
        if !server.is_empty() { spec_cmd.args(["-p", &server]); }
        if !user.is_empty() { spec_cmd.args(["-u", &user]); }
        spec_cmd.args(["client", "-o", &name]);
        let (mut stream, mut root) = (String::new(), String::new());
        if let Ok(spec_out) = spec_cmd.output() {
            let spec = String::from_utf8_lossy(&spec_out.stdout);
            for sline in spec.lines() {
                if let Some(v) = sline.strip_prefix("Stream:") {
                    stream = extract_stream_name(v).unwrap_or_default();
                } else if let Some(v) = sline.strip_prefix("Root:") {
                    root = v.trim().to_string();
                }
            }
        }
        workspaces.push(P4Workspace { name, stream, root });
    }
    Ok(workspaces)
}

#[tauri::command]
pub fn set_p4_connection(server: String, user: String, client: String) {
    set_override(Some(P4Override { server, user, client }));
}

#[tauri::command]
pub fn clear_p4_connection() {
    set_override(None);
}

#[tauri::command]
pub fn check_stale_revisions(pattern: String) -> Result<Vec<String>, String> {
    let output = p4_cmd()
        .args(["fstat", "-T", "depotFile,haveRev,headRev", &pattern])
        .output()
        .map_err(|e| format!("p4 fstat failed: {e}"))?;
    if !output.status.success() { return Ok(Vec::new()); }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut stale = Vec::new();
    let (mut depot_file, mut have_rev, mut head_rev): (Option<String>, Option<i64>, Option<i64>) = (None, None, None);
    let mut flush = |df: &mut Option<String>, hv: &mut Option<i64>, hd: &mut Option<i64>| {
        if let (Some(d), Some(h), Some(r)) = (df.as_ref(), hv.as_ref(), hd.as_ref()) {
            if h < r { stale.push(format!("{} (local #{} < depot #{})", d, h, r)); }
        }
        *df = None; *hv = None; *hd = None;
    };
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { flush(&mut depot_file, &mut have_rev, &mut head_rev); continue; }
        if let Some(v) = line.strip_prefix("... depotFile ") { depot_file = Some(v.to_string()); }
        else if let Some(v) = line.strip_prefix("... haveRev ") { have_rev = v.parse().ok(); }
        else if let Some(v) = line.strip_prefix("... headRev ") { head_rev = v.parse().ok(); }
    }
    flush(&mut depot_file, &mut have_rev, &mut head_rev);
    Ok(stale)
}

#[tauri::command]
pub fn check_concurrent_edits(pattern: String) -> Result<Vec<String>, String> {
    let output = p4_cmd()
        .args(["opened", "-a", &pattern])
        .output()
        .map_err(|e| format!("p4 opened -a failed: {e}"))?;
    if !output.status.success() { return Ok(Vec::new()); }

    let our_client = {
        let info_out = p4_cmd().arg("info").output().ok();
        info_out.and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            s.lines().find_map(|l| l.strip_prefix("Client name:").map(|v| v.trim().to_string()))
        }).unwrap_or_default()
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut conflicts = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let parts: Vec<&str> = line.splitn(2, " by ").collect();
        if parts.len() < 2 { continue; }
        let who = parts[1].split(' ').next().unwrap_or("");
        let client_name = who.splitn(2, '@').nth(1).unwrap_or("");
        if !our_client.is_empty() && client_name == our_client { continue; }
        let depot_path = parts[0].split('#').next().unwrap_or(parts[0]).trim();
        conflicts.push(format!("{} ({})", depot_path, who));
    }
    Ok(conflicts)
}

pub fn resolve_local_path(depot_path: &str) -> Option<String> {
    let output = p4_cmd().args(["where", depot_path]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().and_then(|l| l.split_whitespace().last().map(|s| s.to_string()))
}

#[derive(serde::Serialize, Clone)]
pub struct P4FileChange {
    pub depot_path: String,
    pub local_path: String,
    pub action: String,
}

#[derive(serde::Serialize)]
pub struct P4PendingChanges {
    pub files: Vec<P4FileChange>,
}

#[tauri::command]
pub fn get_p4_pending(pattern: String) -> Result<P4PendingChanges, String> {
    let mut files = Vec::new();
    if let Ok(output) = p4_cmd().args(["opened", &pattern]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let parts: Vec<&str> = line.splitn(4, " - ").collect();
            if parts.len() < 2 { continue; }
            let depot_path = parts[0].split('#').next().unwrap_or(parts[0]).to_string();
            let action = parts[1].trim().split_whitespace().next().unwrap_or("edit").to_string();
            let local_path = resolve_local_path(&depot_path).unwrap_or_default();
            files.push(P4FileChange { depot_path, local_path, action });
        }
    }
    Ok(P4PendingChanges { files })
}

#[derive(serde::Serialize)]
pub struct P4FileDiff {
    pub file: String,
    pub diff: String,
}

#[tauri::command]
pub fn get_p4_diff(file_path: String, action: String) -> Result<P4FileDiff, String> {
    let path = PathBuf::from(&file_path);
    let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(file_path.clone());

    if action == "add" {
        let content = std::fs::read_to_string(&path).map_err(|e| format!("read failed: {e}"))?;
        let lines: Vec<String> = content.lines().map(|l| format!("+{l}")).collect();
        let diff = format!("--- /dev/null\n+++ {}\n@@ -0,0 +1,{} @@\n{}", file, lines.len(), lines.join("\n"));
        return Ok(P4FileDiff { file, diff });
    }

    let output = p4_cmd().args(["diff", "-du", &file_path]).output()
        .map_err(|e| format!("p4 diff failed: {e}"))?;
    Ok(P4FileDiff { file, diff: String::from_utf8_lossy(&output.stdout).to_string() })
}

pub fn p4_edit(path: &str) -> Result<(), String> {
    let output = p4_cmd().args(["edit", path]).output().map_err(|e| format!("p4 edit: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

pub fn p4_add(path: &str) -> Result<(), String> {
    let output = p4_cmd().args(["add", path]).output().map_err(|e| format!("p4 add: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

pub fn p4_revert_unchanged(pattern: &str) -> Result<(), String> {
    let output = p4_cmd().args(["revert", "-a", pattern]).output().map_err(|e| format!("p4 revert -a: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

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

/// SL-17200 (v0.3.4/v0.3.5): P4 stdout 의 인코딩이 OS / P4 client / stdio
/// 종류에 따라 달라 fallback chain 으로 처리:
///   1. UTF-8 valid 이면 그대로 (macOS / Linux / P4 가 UTF-8 emit 하는 Windows).
///   2. Windows 에서 `MultiByteToWideChar(CP_ACP, ...)` 로 system codepage
///      (CP949 등) decode 시도.
///   3. Windows 에서 OEM codepage fallback (console default 와 다른 경우).
///   4. 최후 fallback — `from_utf8_lossy` (깨진 문자라도 panic 없이).
///
/// UTF-8 first 는 `-Mj` JSON output 이 Windows pipe stdio 에서도 raw UTF-8
/// bytes 일 가능성 (`chcp 65001` 후 정상 표시되는 것이 console encoding
/// 변경 vs P4 출력 인코딩 변경 어느 쪽인지 모호 — UTF-8 valid 검사가
/// 가장 안전).
pub fn decode_p4_stdout(bytes: &[u8]) -> String {
    // Step 1: UTF-8 valid?
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    // Step 2/3: Windows codepage fallback
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::{CP_ACP, CP_OEMCP};
        if let Some(s) = decode_with_codepage(bytes, CP_ACP) {
            // CP_ACP decode 결과가 또 invalid (e.g., replacement char 가득) 면
            // OEM 시도. 다만 단순 fallback — replacement char 있어도 일단 반환.
            return s;
        }
        if let Some(s) = decode_with_codepage(bytes, CP_OEMCP) {
            return s;
        }
    }

    // Step 4: lossy UTF-8
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(windows)]
fn decode_with_codepage(bytes: &[u8], codepage: u32) -> Option<String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Globalization::MultiByteToWideChar;
    if bytes.is_empty() { return Some(String::new()); }
    let wide_len = unsafe {
        MultiByteToWideChar(
            codepage, 0,
            bytes.as_ptr() as _, bytes.len() as i32,
            std::ptr::null_mut(), 0,
        )
    };
    if wide_len <= 0 { return None; }
    let mut buf = vec![0u16; wide_len as usize];
    let written = unsafe {
        MultiByteToWideChar(
            codepage, 0,
            bytes.as_ptr() as _, bytes.len() as i32,
            buf.as_mut_ptr(), wide_len,
        )
    };
    if written <= 0 { return None; }
    buf.truncate(written as usize);
    Some(
        std::ffi::OsString::from_wide(&buf)
            .to_string_lossy()
            .into_owned(),
    )
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
                if let Some(s) = parse_stream_from_spec(&decode_p4_stdout(&out.stdout)) {
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
        Some(decode_p4_stdout(&output.stdout).to_string())
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

    let stdout = decode_p4_stdout(&output.stdout);
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
            let spec = decode_p4_stdout(&spec_out.stdout);
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
    if !output.status.success() {
        // 비정상 종료 (인증 만료 / client lock / server 부하 등) 를 silent 로
        // 빈 배열 변환하면 caller 측 store 가 "0건 검출" 로 오인해 stale 표시가
        // 사라지는 회귀 (SL-17195). stderr 포함해 Err 로 전파 — 호출 측은
        // 이전 값을 유지하는 정책으로 처리.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "p4 fstat non-zero exit (code={:?}): {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    let stdout = decode_p4_stdout(&output.stdout);

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
    if !output.status.success() {
        // SL-17195: 비정상 종료 silent 변환 금지 — `check_stale_revisions` 와
        // 동일 정책. stderr 포함해 Err 로 전파.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "p4 opened -a non-zero exit (code={:?}): {}",
            output.status.code(),
            stderr.trim()
        ));
    }

    let our_client = {
        let info_out = p4_cmd().arg("info").output().ok();
        info_out.and_then(|o| {
            let s = decode_p4_stdout(&o.stdout).to_string();
            s.lines().find_map(|l| l.strip_prefix("Client name:").map(|v| v.trim().to_string()))
        }).unwrap_or_default()
    };

    let stdout = decode_p4_stdout(&output.stdout);
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

/// 사용자가 미동기화한 submitted changelist 메타데이터.
/// v0.3.2 (MetadataEditor SL-17196) — 외부 변경 감지 UI 가 file 목록 대신
/// CL 목록을 표시할 때 사용.
///
/// `user_fullname` 은 `p4 users` 의 결과로 lookup 한 사람 이름 (e.g. "임종현").
/// lookup 실패 시 `user` (id) 와 동일.
///
/// `time` 은 Unix epoch (seconds) — caller (frontend) 가 locale-aware 변환.
/// Rust 측 자체 date format 변환 안 함 (chrono 의존 회피 + locale 선택은 UI 책임).
///
/// v0.3.3 (SL-17200) — Windows console codepage 인코딩 깨짐 회피를 위해
/// `p4 changes -Mj -ztag` JSON output 사용. 한글 description / FullName 정상.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct P4Change {
    pub number: i64,
    pub user: String,
    pub user_fullname: String,
    pub client: String,
    pub time: i64,           // Unix epoch seconds
    pub description: String, // first non-empty line, trimmed
}

/// `p4 -Mj -ztag users` → `{user_id → FullName}` 맵. 실패 시 빈 맵.
/// JSON output: `{"User":"id","FullName":"...","Email":"...",...}` line-by-line.
/// v0.3.3 (SL-17200): 텍스트 parse → JSON (Windows console codepage 인코딩 깨짐 회피).
fn fetch_user_fullnames() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let out = match p4_cmd().args(["-Mj", "-ztag", "users"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return map,
    };
    let s = decode_p4_stdout(&out.stdout);
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let user = match v.get("User").and_then(|x| x.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let fullname = v.get("FullName").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if !fullname.is_empty() {
            map.insert(user, fullname);
        }
    }
    map
}

/// `pattern` (예: `<dataDir>/...`) 의 stale 파일들에 영향을 준 submitted
/// changelist 목록을 반환. file 단위 stale 표시보다 의미 압축 — "어느 CL 이
/// 아직 sync 안 됐는지 + 누가 무엇을 했는지" 를 한 화면에.
///
/// 알고리즘:
///   1. `p4 fstat -F headChange>haveChange -T headChange <pattern>` — stale 인
///      파일들의 headChange 집합 추출 (P4 의 `@have+1` 표기 미지원 우회).
///   2. min(headChange) 부터 `#head` 까지의 changes 를 `p4 changes -l -s
///      submitted <pattern>@<min>,#head` 로 받음.
///   3. parse 후 changeNumber ∈ stale headChange 집합 인 항목만 filter.
///
/// stale 가 없으면 빈 `Vec` 반환. P4 명령 비정상 종료는 stderr 포함 `Err` 로
/// 전파 (v0.3.1 패턴).
#[tauri::command]
pub fn list_pending_changes(pattern: String) -> Result<Vec<P4Change>, String> {
    // Step 1: 모든 파일의 (haveRev, headRev, headChange) 받기 + client side filter.
    //
    // 주의: `haveChange` attribute 는 P4 server / protocol 설정에 따라
    // 출력 안 될 수 있어 stale 판정에 신뢰 못 함 (실제 환경에서 누락 확인).
    // `haveRev / headRev` 는 항상 출력 → check_stale_revisions 와 동일한
    // rev 기반 판정으로 통일. stale 파일의 headChange 를 별도로 받아 CL
    // 집합 구성.
    let fstat_out = p4_cmd()
        .args(["fstat", "-T", "depotFile,haveRev,headRev,headChange", &pattern])
        .output()
        .map_err(|e| format!("p4 fstat (for changes) failed: {e}"))?;
    if !fstat_out.status.success() {
        let stderr = String::from_utf8_lossy(&fstat_out.stderr);
        return Err(format!(
            "p4 fstat (for changes) non-zero exit (code={:?}): {}",
            fstat_out.status.code(),
            stderr.trim()
        ));
    }
    let fstat_str = decode_p4_stdout(&fstat_out.stdout);
    let mut head_changes: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut cur_have_rev: Option<i64> = None;
    let mut cur_head_rev: Option<i64> = None;
    let mut cur_head_change: Option<i64> = None;
    let flush_record = |have_rev: &mut Option<i64>,
                        head_rev: &mut Option<i64>,
                        head_change: &mut Option<i64>,
                        out: &mut std::collections::HashSet<i64>| {
        if let (Some(hv), Some(hr), Some(hc)) = (*have_rev, *head_rev, *head_change) {
            // stale = 기존 sync 한 파일인데 새 revision 있음 (haveRev < headRev).
            // 신규 파일 (haveRev None) 은 skip — check_stale_revisions 와 일관.
            // 신규 파일을 포함하면 각각 다른 CL 에서 만들어진 경우가 많아 head_changes
            // 집합이 폭발 → CL 수가 stale 파일 수보다 훨씬 커지는 결과 (사용자 보고:
            // 196 stale → 519 CL). 신규 파일 분리는 V1.x 후속 검토.
            if hv < hr { out.insert(hc); }
        }
        *have_rev = None;
        *head_rev = None;
        *head_change = None;
    };
    for line in fstat_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush_record(&mut cur_have_rev, &mut cur_head_rev, &mut cur_head_change, &mut head_changes);
            continue;
        }
        if let Some(v) = line.strip_prefix("... haveRev ") {
            cur_have_rev = v.parse().ok();
        } else if let Some(v) = line.strip_prefix("... headRev ") {
            cur_head_rev = v.parse().ok();
        } else if let Some(v) = line.strip_prefix("... headChange ") {
            cur_head_change = v.parse().ok();
        }
    }
    flush_record(&mut cur_have_rev, &mut cur_head_rev, &mut cur_head_change, &mut head_changes);
    if head_changes.is_empty() { return Ok(Vec::new()); }

    // Step 2: changes range 받기 (JSON output — Windows codepage 우회).
    let min_change = *head_changes.iter().min().unwrap();
    let range_pattern = format!("{}@{},#head", pattern, min_change);
    let changes_out = p4_cmd()
        .args(["-Mj", "-ztag", "changes", "-l", "-s", "submitted", &range_pattern])
        .output()
        .map_err(|e| format!("p4 changes failed: {e}"))?;
    if !changes_out.status.success() {
        let stderr = String::from_utf8_lossy(&changes_out.stderr);
        return Err(format!(
            "p4 changes non-zero exit (code={:?}): {}",
            changes_out.status.code(),
            stderr.trim()
        ));
    }
    let changes_str = decode_p4_stdout(&changes_out.stdout);
    let parsed = parse_p4_changes(&changes_str);

    // Step 3: user FullName lookup (failures fallback to user id).
    let fullnames = fetch_user_fullnames();

    // Step 4: stale headChange set 에 속한 CL 만 (다른 path 가 같은 range 에 있어도
    // 그 변경이 stale 한 metadata 파일에 영향 준 게 아니면 제외) + fullname 채움.
    let filtered: Vec<P4Change> = parsed
        .into_iter()
        .filter(|c| head_changes.contains(&c.number))
        .map(|mut c| {
            c.user_fullname = fullnames.get(&c.user).cloned().unwrap_or_else(|| c.user.clone());
            c
        })
        .collect();
    Ok(filtered)
}

/// `p4 -Mj -ztag changes -l` 의 JSON line output 파싱. line 당 하나의 change.
/// 형식: `{"change":"12345","user":"id","client":"...","desc":"...","time":"<unix>",...}`
/// v0.3.3 (SL-17200) — 텍스트 parse → JSON (Windows console codepage 깨짐 회피).
fn parse_p4_changes(s: &str) -> Vec<P4Change> {
    let mut out = Vec::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let number: i64 = v.get("change")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if number == 0 { continue; }
        let user = v.get("user").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let client = v.get("client").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let time: i64 = v.get("time")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let desc_full = v.get("desc").and_then(|x| x.as_str()).unwrap_or("");
        let description = desc_full
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();
        out.push(P4Change {
            number,
            user,
            user_fullname: String::new(),
            client,
            time,
            description,
        });
    }
    out
}

#[cfg(test)]
mod tests_p4_changes {
    use super::*;

    #[test]
    fn parse_single_change_json() {
        let s = r#"{"change":"12345","user":"jonghyun","client":"my_client","desc":"First line.\nSecond.","time":"1778499509"}"#;
        let r = parse_p4_changes(s);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].number, 12345);
        assert_eq!(r[0].user, "jonghyun");
        assert_eq!(r[0].client, "my_client");
        assert_eq!(r[0].time, 1778499509);
        assert_eq!(r[0].description, "First line.");
    }

    #[test]
    fn parse_multiple_changes_json() {
        let s = "{\"change\":\"100\",\"user\":\"a\",\"client\":\"c1\",\"desc\":\"desc a\",\"time\":\"1\"}\n{\"change\":\"101\",\"user\":\"b\",\"client\":\"c2\",\"desc\":\"desc b\",\"time\":\"2\"}\n";
        let r = parse_p4_changes(s);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].number, 100);
        assert_eq!(r[0].description, "desc a");
        assert_eq!(r[1].number, 101);
        assert_eq!(r[1].description, "desc b");
    }

    #[test]
    fn parse_empty() {
        let r = parse_p4_changes("");
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn parse_malformed_line_skipped() {
        // JSON 이 아닌 라인은 skip. number=0 인 record 도 skip.
        let s = "garbage line\n{\"change\":\"\",\"user\":\"x\"}\n{\"change\":\"42\",\"user\":\"u\",\"client\":\"c\",\"desc\":\"ok\",\"time\":\"1\"}\n";
        let r = parse_p4_changes(s);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].number, 42);
    }

    #[test]
    fn parse_korean_description_preserved() {
        // SL-17200 회귀 방지 — JSON output 이 한글 정상 보존.
        let s = r#"{"change":"7","user":"u","client":"c","desc":"[기획][서영오] 월드 보스 (50 -> 30)","time":"1"}"#;
        let r = parse_p4_changes(s);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].description, "[기획][서영오] 월드 보스 (50 -> 30)");
    }
}

pub fn resolve_local_path(depot_path: &str) -> Option<String> {
    let output = p4_cmd().args(["where", depot_path]).output().ok()?;
    let stdout = decode_p4_stdout(&output.stdout);
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
        let stdout = decode_p4_stdout(&output.stdout);
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
    Ok(P4FileDiff { file, diff: decode_p4_stdout(&output.stdout).to_string() })
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

/// MS.4a: file 을 p4 pending CL 에 delete 표시. 성공 시 로컬 파일도 제거됨.
/// 실패 (p4 미연결 / 미트래킹 등) 면 호출측이 OS 삭제로 fallback.
pub fn p4_delete(path: &str) -> Result<(), String> {
    let output = p4_cmd().args(["delete", path]).output().map_err(|e| format!("p4 delete: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

/// MS.4a: file 을 unopened 상태로 되돌림 (어떤 pending action 이든). 이미 unopened
/// 면 noop (실패 무시). p4 edit 상태 파일을 delete 하려면 먼저 revert 해야 하므로.
pub fn p4_revert(path: &str) -> Result<(), String> {
    let output = p4_cmd().args(["revert", path]).output().map_err(|e| format!("p4 revert: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

/// `p4 protects -m //depot/...` 으로 사용자의 max access level 을 반환.
///   - "list" / "read" / "open" / "write" / "review" / "admin" / "super" / "none"
///   - 명령 실패 / 출력 비어있음 → "none" (안전 기본값)
///   - p4 미설정 / 서버 미연결 → Err 반환 (caller 가 처리)
pub fn p4_max_protect(depot_path: &str) -> Result<String, String> {
    let output = p4_cmd()
        .args(["protects", "-m", depot_path])
        .output()
        .map_err(|e| format!("p4 protects: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // "no protections defined" 류는 read 도 없는 상태로 처리 (none)
        if stderr.contains("no protections") || stderr.contains("no permission") {
            return Ok("none".to_string());
        }
        return Err(stderr);
    }
    let level = decode_p4_stdout(&output.stdout).trim().to_string();
    Ok(if level.is_empty() { "none".to_string() } else { level })
}

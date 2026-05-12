//! 도구 자신의 P4 binary 가 stale 일 때 한 번의 액션으로 sync + 재시작. Windows 한정.
//!
//! 핵심 — running .exe 의 Windows file lock 우회:
//!   1. running .exe 를 같은 디렉토리의 '.old' 로 rename.
//!      NT 의 PE loader 가 memory-mapped 로 로드해 두므로 파일 이름은
//!      directory entry 뿐 — rename 은 항상 성공.
//!   2. 원래 경로가 비었으니 `p4 sync` 가 새 파일 작성 가능.
//!   3. 새 binary 를 detached 로 spawn → 현재 프로세스 exit.
//!   4. 다음 시작 시 `.old` 정리 (best-effort).
//!
//! 실패 시: rename 복구 (`.old` → 원본) 후 에러 반환.
//!
//! macOS / Linux: 같은 rename 트릭이 동작하지만 호출자 (도구 측 frontend) 가
//! production deploy 경로 패턴 매칭으로 진입을 가드하는 것이 일반적이다.
//! 여기서는 OS 만 검사하고 진행은 호출자 책임.
//!
//! Deploy 경로 검사 등 도구별 가드는 모두 호출자 측. 이 crate 는 plain wrapper.

use crate::p4_cmd;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

const OLD_SUFFIX: &str = ".old";

#[derive(Serialize, Default, Debug)]
pub struct UpdateResult {
    /// sync 가 갱신한 파일 라인 (원본 `p4 sync` 출력 그대로).
    pub sync_updated: Vec<String>,
    /// sync 결과 에러 라인 (stderr 의 up-to-date 이외).
    pub sync_errors: Vec<String>,
    /// raw stderr — 디버깅 / 상세 보기.
    pub raw_stderr: String,
}

/// 현재 실행 중 binary 의 절대 경로.
pub fn get_current_binary_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("current_exe 조회 실패: {e}"))
}

/// 이전 update 사이클이 남긴 `<exe>.old` 파일 정리. 보통 시작 시 1회 호출.
/// 실패는 silent (다음 update 사이클이 다시 시도). 반환: 실제로 지웠으면 true.
pub fn cleanup_stale_binary() -> Result<bool, String> {
    let cur = get_current_binary_path()?;
    let old = with_old_suffix(&cur);
    if !old.exists() {
        return Ok(false);
    }
    match std::fs::remove_file(&old) {
        Ok(()) => Ok(true),
        Err(e) => {
            // antivirus / sharing violation 가능 — silent
            eprintln!("[self_update] cleanup_stale_binary remove failed: {e}");
            Ok(false)
        }
    }
}

/// Windows: running .exe 를 .old 로 rename → `p4 sync <exe path>` → 새 binary
/// spawn detached → 현재 프로세스 exit.
///
/// 반환: spawn 직전까지의 sync 결과. exit 후엔 frontend 가 사라지므로 사용자는
/// 보지 못함 — 디버깅 / 로그 용.
///
/// macOS / Linux 는 미지원 — 사용자는 보통 dev rebuild 로 갱신.
#[allow(unused_variables, unused_mut)]
pub fn p4_update_and_restart(app: &AppHandle) -> Result<UpdateResult, String> {
    let cur = get_current_binary_path()?;
    let old = with_old_suffix(&cur);

    if !cfg!(windows) {
        return Err(
            "자기 업데이트는 Windows 전용. macOS/Linux 는 dev rebuild 또는 수동 p4 sync 사용."
                .to_string(),
        );
    }

    // 1. 기존 .old 가 있으면 먼저 정리 (이전 update 잔재).
    if old.exists() {
        let _ = std::fs::remove_file(&old);
    }

    // 2. running binary → .old 로 rename.
    std::fs::rename(&cur, &old)
        .map_err(|e| format!("binary rename 실패 (running .exe → .old): {e}"))?;

    // 3. p4 sync 로 원래 경로에 새 binary 작성.
    let mut cmd = p4_cmd();
    cmd.arg("sync").arg(cur.to_string_lossy().to_string());
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            // rename 복구 후 에러 반환.
            let _ = std::fs::rename(&old, &cur);
            return Err(format!("p4 sync 실행 실패: {e}"));
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // 4. 결과 분류. 새 파일 미작성이면 rename 복구.
    let (sync_updated, sync_errors) = classify_sync(&stdout, &stderr);
    if !cur.exists() {
        let _ = std::fs::rename(&old, &cur);
        return Err(format!(
            "p4 sync 후에도 binary 가 없음. errors={}, stderr={}",
            sync_errors.len(),
            stderr.trim()
        ));
    }
    let result = UpdateResult { sync_updated, sync_errors, raw_stderr: stderr };

    // 5. 새 binary 를 detached 로 spawn.
    if let Err(e) = spawn_detached(&cur) {
        return Err(format!("새 binary spawn 실패 (수동 재실행 필요): {e}"));
    }

    // 6. 현재 프로세스 graceful exit — Tauri runtime cleanup 포함.
    app.exit(0);
    Ok(result)
}

fn with_old_suffix(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(OLD_SUFFIX);
    PathBuf::from(s)
}

/// Windows: CREATE_NEW_PROCESS_GROUP + DETACHED_PROCESS 로 부모와 완전 분리.
fn spawn_detached(path: &Path) -> std::io::Result<()> {
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP (0x00000200) + DETACHED_PROCESS (0x00000008)
        cmd.creation_flags(0x00000200 | 0x00000008);
    }
    cmd.spawn()?;
    Ok(())
}

/// `p4 sync` 출력 라인 분류. self_update 내부 용도 — sync 모듈의 generic 분류는
/// 도구 측 책임이지만 self_update 는 결과를 UpdateResult 로 자체 노출하므로 여기에
/// 최소 분류 유지.
fn classify_sync(stdout: &str, stderr: &str) -> (Vec<String>, Vec<String>) {
    let mut updated = Vec::new();
    let mut errors = Vec::new();
    for line in stdout.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if l.contains(" - updating ")
            || l.contains(" - added as ")
            || l.contains(" - refreshing ")
        {
            updated.push(l.to_string());
        }
    }
    for line in stderr.lines() {
        let l = line.trim();
        if l.is_empty() || l.contains("file(s) up-to-date") {
            continue;
        }
        errors.push(l.to_string());
    }
    (updated, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_old_suffix_appends() {
        assert_eq!(
            with_old_suffix(Path::new("/x/my-tool.exe")),
            PathBuf::from("/x/my-tool.exe.old")
        );
    }

    #[test]
    fn classify_sync_basic() {
        let stdout = "//depot/release/my-tool.exe#13 - updating /local/my-tool.exe\n";
        let stderr = "";
        let (u, e) = classify_sync(stdout, stderr);
        assert_eq!(u.len(), 1);
        assert!(e.is_empty());
    }

    #[test]
    fn classify_sync_error_propagates() {
        let stdout = "";
        let stderr = "//depot/release/my-tool.exe - no such file";
        let (u, e) = classify_sync(stdout, stderr);
        assert!(u.is_empty());
        assert_eq!(e.len(), 1);
    }

    #[test]
    fn classify_sync_skips_up_to_date() {
        let stdout = "";
        let stderr = "//depot/release/my-tool.exe - file(s) up-to-date.";
        let (u, e) = classify_sync(stdout, stderr);
        assert!(u.is_empty());
        assert!(e.is_empty());
    }
}

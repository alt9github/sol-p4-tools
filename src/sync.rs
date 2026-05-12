//! `p4 sync` raw 실행 wrapper. 결과 분류 (updated / already_current / errors) 는
//! 도구 측 — UI 표현 방식이 도구마다 달라 강제 구조화 회피.
//!
//! 빈 paths 는 즉시 `Ok(empty)`.

use crate::p4_cmd;

/// raw stdout / stderr 만 반환. 라인 분류는 호출자가.
#[derive(Default, Debug)]
pub struct P4SyncRawOutput {
    pub stdout: String,
    pub stderr: String,
}

/// `p4 sync <path1> <path2> ...`. paths 는 depot 또는 local 양쪽 허용 (P4 가 해석).
pub fn p4_sync(paths: &[String]) -> Result<P4SyncRawOutput, String> {
    if paths.is_empty() {
        return Ok(P4SyncRawOutput::default());
    }
    let mut cmd = p4_cmd();
    cmd.arg("sync");
    for p in paths {
        cmd.arg(p);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("p4 sync 실행 실패: {e}"))?;
    Ok(P4SyncRawOutput {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_paths_returns_empty() {
        let r = p4_sync(&[]).unwrap();
        assert!(r.stdout.is_empty());
        assert!(r.stderr.is_empty());
    }
}

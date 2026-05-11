# Changelog

## v0.3.1 (2026-05-11)

P4 명령 비정상 종료를 silent 로 흡수하던 동작 정정 — 호출 측이 "검사 실패"
와 "0건 검출" 을 구분할 수 있도록 stderr 포함한 `Err` 전파.

### 배경 (MetadataEditor SL-17195)

`check_stale_revisions` / `check_concurrent_edits` 가 P4 명령 비정상 종료
(인증 ticket 만료 / client lock / server 부하 등) 시 `Ok(Vec::new())` 로
silent 반환했음. 호출 측 (MetadataEditor) 의 store 가 "0건 검출" 로 오인해
"외부 변경" 표시가 사라지는 회귀로 이어짐. (실제 SL-17195 의 main cause 는
호출 측 pattern 생성 버그였지만, 본 silent 동작은 잠재 위험으로 별도 정정.)

### Rust crate (`sol-p4-tools`)

- `p4::check_stale_revisions` — `!output.status.success()` 시 `Err(format!("p4
  fstat non-zero exit (code={:?}): {stderr}"))` 로 전파. 정상 종료 + 빈
  결과는 그대로 `Ok(Vec::new())` 유지.
- `p4::check_concurrent_edits` — 동일 패턴. `p4 opened -a` 비정상 종료 시
  Err 전파.

### Breaking 가능성

호출 측이 이전에 `Err` 를 catch 하지 않았다면 (실패가 빈 배열로 흡수된다는
전제) — 이제 throw 됨. MetadataEditor 의 V1.B.1 후속에서 catch wrap 도입
되어 안전 (SL-17195 fix 의 (C) 슬라이스).

## v0.3.0 (2026-04-28)

P4 protections + delete/revert helpers. Backported from MetadataEditor (CL 202722) — used by the new "MetaData 폴더 권한 기반 read-only 게이팅" feature.

### Rust crate (`sol-p4-tools`)
- `p4::p4_max_protect(depot_path)` — query the user's max access level via `p4 protects -m <path>`. Returns `"list"`/`"read"`/`"open"`/`"write"`/`"review"`/`"admin"`/`"super"` or `"none"` (mapped from `no protections defined` / `no permission` stderr). Caller maps to UI gating tiers (write / read / none / unknown).
- `p4::p4_delete(path)` — mark file for delete in the pending CL. Used by apps that surface a "delete this file" action under Perforce control (e.g., View Designer 의 view 파일 삭제).
- `p4::p4_revert(path)` — revert any pending action; prerequisite for delete-after-edit flows where a file might already be open for edit before the user requests deletion.

### TypeScript package (`@alt9github/sol-p4-tools`)
- `ts/package.json` — added `vitest` devDependency + `test` / `test:watch` / `typecheck` scripts.
- `ts/vitest.config.ts` — minimal node-environment vitest config so the TS package can run its own tests in line with the Rust crate's `cargo test`.

> Note: this CHANGELOG entry was committed to `main` after the `v0.3.0` tag was published — the tag itself does not include this entry. Future releases (`v0.3.1+`) will have CHANGELOG land in the same commit as the version bump.

## v0.2.0 (2026-04-22)

Windows UX + branch-detection robustness, environment diagnostics. Backported from LevelMetadataEditor.

### Rust crate (`sol-p4-tools`)
- `p4::p4_bare()` — new helper that builds a bare `p4` Command and applies the Windows `CREATE_NO_WINDOW` (0x08000000) creation flag. Every p4 spawn now routes through it (`p4_cmd` / `get_p4_stream` override branch / `list_p4_workspaces` x2). Eliminates the console window that flashed on every p4 subprocess invocation when the app was launched from a GUI (Tauri webview) parent on Windows.
- `workspace.rs` — detection order reworked to **exe dir → cwd → `p4 info`**. Windows shortcut launches (where cwd may be `C:\Program Files\Perforce` etc.) now pin to the correct branch based on the executable location, not the launch cwd. New public helpers: `find_p4config_root`, `find_branch_root`, `exe_dir`.
- `workspace::detect_data_dir` — Tauri command moved into the crate (previously lived in consumer apps). Same exe-first detection order; returns the active branch's `Deploy/GeneratedData_Server` path as `Option<String>`.
- `diagnostics` — new module. `collect(app_version: &str) -> Diagnostics` returns a structured snapshot used by Settings → Diagnostics panels: resolved project root / data dir, p4 info parsed into `p4_client_name/root/stream`, `client_matches_exe` flag (component-wise path comparison, case-insensitive on Windows — avoids `C:\foo` vs `C:\foo_bar` false-match), and per-candidate (exe, cwd) ancestor analysis (`.p4config`, `MetaData/Schema`, `Deploy/...`). Exposed as a library function so each consumer registers a thin Tauri-command wrapper that captures its own `CARGO_PKG_VERSION`.

### Fixes
- `check_stale_revisions` — closure captured `Option<i64>` fields by move where it needed borrow; fixed `(df.as_ref(), hv.as_ref(), hd.as_ref())` so successive flush calls compile and work.

## v0.1.0 (2026-04-17)

Initial scaffolding — extracted from LevelMetadataEditor and RewardEditor.

### Rust crate (`sol-p4-tools`)
- `p4.rs` — P4 command builder with override support, stream detection (multi-strategy), workspace listing, connection management, stale revision check, concurrent edit check, pending changes, diff, edit/add/revert helpers
- `workspace.rs` — P4 root detection (.p4config / p4 info), project root discovery (multi-strategy)
- `metadata_io.rs` — BOM-aware JSON read/write, partition directory loader, atomic file write (tmp → rename)
- `partition.rs` — SHA1-based 16-way partition postfix computation (DataTool compatible)

### TypeScript package (`@alt9github/sol-p4-tools`)
- `p4-client.ts` — Typed Tauri invoke wrappers for all Rust commands + P4 error categorization (7 error kinds with Korean messages)

### Example
- Minimal Tauri v2 demo app exercising all P4 commands — connection setup, stream detection, pending files, stale/concurrent checks, diff viewer, error categorization test

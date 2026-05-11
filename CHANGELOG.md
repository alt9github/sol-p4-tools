# Changelog

## v0.3.5 (2026-05-11)

`decode_p4_stdout` 의 decode chain 을 UTF-8 first + CP_ACP / CP_OEMCP
fallback 으로 수정 — v0.3.4 의 CP_ACP 만 decode 가 UTF-8 bytes 를 CP949 로
잘못 해석하던 회귀 정정.

### 배경 (MetadataEditor SL-17200 후속2)

사용자 Windows release 빌드 (`chcp 949`) 에서 v0.3.4 적용 후 깨진 패턴이
`占썼영占쏙옙[占쏙옙획]` 형태로 변화. 이는 **UTF-8 bytes 가 CP_ACP (CP949)
로 잘못 decode** 된 전형 — UTF-8 `기` (`\xEA\xB8\xB0`) 의 첫 2 byte
`\xEA\xB8` 가 CP949 에서 `占` 한자에 매핑되는 특징.

→ Windows P4 client 가 pipe stdio (Rust Command 의 stdout) 에서는
**UTF-8 emit**. console output 만 codepage (CP949) 변환 적용. v0.3.4 의
CP_ACP only decode 는 잘못된 가정.

### Rust crate (`sol-p4-tools`)

- `decode_p4_stdout` 의 chain 을 4 단계로 변경:
  1. `std::str::from_utf8` 으로 UTF-8 valid 검사 → success 시 그대로 (대부분
     케이스).
  2. Windows + UTF-8 invalid 시 `MultiByteToWideChar(CP_ACP, ...)` 로 system
     codepage decode.
  3. Windows + CP_ACP 도 실패 시 `CP_OEMCP` fallback.
  4. 최후 `from_utf8_lossy` (panic 회피).
- `decode_with_codepage` private helper 추출.

### Breaking

없음 (chain 추가만, 기존 시그니처 유지).

## v0.3.4 (2026-05-11)

Windows 의 P4 client stdout codepage 변환 우회 — `-Mj` JSON output 도 system
ACP (CP949 등) 로 변환되어 깨졌던 한글이 정상 표시.

### 배경 (MetadataEditor SL-17200 후속)

v0.3.3 의 `-Mj -ztag` JSON output 도 Windows 에서 한글 mojibake.
`chcp 65001` 후 정상 = **P4 client 가 stdout 에 console output codepage
적용**. Rust 의 `from_utf8_lossy` 가 CP949 bytes 를 UTF-8 로 해석 시 깨짐.
`P4 server` 가 non-unicode 모드라 `-C utf8` / `P4COMMANDCHARSET` 등도 효과 X.

### Rust crate (`sol-p4-tools`)

- `decode_p4_stdout(bytes)` 함수 신설. Windows 에서 Win32
  `MultiByteToWideChar(CP_ACP, ...)` 호출 → system codepage 정확하게 decode
  → UTF-16 → UTF-8 String. Linux/macOS 는 `from_utf8_lossy` 그대로.
- 모든 P4 stdout 처리 callsite 를 `decode_p4_stdout` 으로 일괄 전환 (10건).
  영향 명령: get_p4_stream / list_p4_workspaces / check_stale_revisions /
  check_concurrent_edits / list_pending_changes / fetch_user_fullnames /
  get_p4_pending / get_p4_diff / p4_max_protect 등.
- `[target.'cfg(windows)'.dependencies]` 로 `windows-sys` (Win32_Globalization
  feature) 추가. Linux/macOS 빌드 의존성 영향 없음.

### Breaking

없음 (`decode_p4_stdout` 은 추가, 기존 함수 시그니처 유지).

## v0.3.3 (2026-05-11)

`p4 changes` / `p4 users` 호출을 `-Mj -ztag` JSON output 으로 전환 —
Windows console codepage (CP949 등) 변환에 따른 한글 description /
FullName 깨짐 (mojibake) 회피.

### 배경 (MetadataEditor SL-17200)

v0.3.2 의 plain text `p4 changes -l` / `p4 users` output 이 Windows release
환경에서 한글 mojibake (`���` 같은 �) 로 표시. P4 server 가 non-unicode
mode 라 client-side codepage 변환이 적용 — macOS (UTF-8) 는 정상, Windows
(CP949) 는 깨짐. Rust 의 `from_utf8_lossy` 가 codepage bytes 를 정상 변환
못 함.

### Rust crate (`sol-p4-tools`)

- `list_pending_changes` 의 `p4 changes` 호출에 `-Mj -ztag` flag 추가.
  output 이 JSON lines — encoding 의존 우회 (P4 server 의 unicode 모드와
  무관하게 UTF-8 보장).
- `fetch_user_fullnames` 의 `p4 users` 도 동일 변경.
- `P4Change.time` 타입 변경: `String` (YYYY/MM/DD) → `i64` (Unix epoch
  seconds). JSON output 의 raw timestamp 그대로 — locale-aware 변환은
  caller (frontend) 책임. chrono dep 회피.
- `parse_p4_changes` 가 plain text multi-line parser → JSON line parser.
  serde_json 사용 (기존 dependency).

### 테스트

- `parse_p4_changes` — single / multi / empty / malformed line skipped /
  **korean description preserved** (SL-17200 회귀 방지) 5 케이스.

### Breaking 가능성

- `P4Change.time` 타입 변경 (string → i64) — caller TS / Rust 코드 영향.
  MetadataEditor 의 SyncPromptDialog / P4Panel UI 가 동시 업데이트
  (Date 변환은 frontend).

## v0.3.2 (2026-05-11)

`list_pending_changes` 추가 — 외부 변경 감지 UI 가 file 목록 대신 changelist
목록 (CL 번호 / 사용자 / description) 으로 표시 가능하게 함.

### 배경 (MetadataEditor SL-17196)

기존 `check_stale_revisions` 가 stale 한 file 목록을 반환 — UI 가 174 file
나열 같은 형태로 표시했음. 사용자 멘탈 모델은 CL 단위 ("누가 어떤 CL 을
sync 안 했는가") 라 file 목록은 정보 압축이 부족.

### Rust crate (`sol-p4-tools`)

- `P4Change` struct (`#[serde(rename_all = "camelCase")]`) — `number / user /
  user_fullname / client / time / description`. `user_fullname` 은 `p4 users`
  로 lookup 한 사람 이름 (e.g., "임종현"). lookup 실패 시 user id 와 동일.
- `p4::list_pending_changes(pattern)` — stale 파일들에 영향을 준 submitted
  changelist 목록. 알고리즘:
  1. `p4 fstat -T "depotFile,haveRev,headRev,headChange" <pattern>` — 모든
     파일 + client side filter (`haveRev < headRev` 인 파일의 `headChange`
     집합). P4 의 `-F` 는 두 attribute 간 비교를 지원 안 해 client 측 필터.
     `haveChange` 는 P4 server/protocol 설정에 따라 출력 안 될 수 있어
     `haveRev / headRev` 기반으로 stale 판정 (check_stale_revisions 와 일관).
     신규 파일 (haveRev None) 은 skip — 신규 파일을 포함하면 head_changes
     집합이 폭발해 CL 수가 stale 파일 수보다 훨씬 커짐.
  2. min(headChange) ~ #head 범위의 changes 받기: `p4 changes -l -s submitted
     <pattern>@<min>,#head`.
  3. parse 후 changeNumber ∈ headChange 집합 인 항목만 filter + user_fullname
     채움.
- `fetch_user_fullnames()` (private) — `p4 users` 결과를 `{user_id →
  FullName}` 맵으로 반환. 형식: `<user> <email> (<FullName>) accessed <date>`.
- 비정상 종료는 stderr 포함 `Err` 전파 (v0.3.1 패턴).

### 테스트

- `parse_p4_changes` — single / multi / empty / malformed header 4 케이스.

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

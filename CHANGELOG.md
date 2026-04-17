# Changelog

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

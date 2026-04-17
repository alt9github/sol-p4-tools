# sol-p4-tools

Perforce (P4) integration toolkit for Tauri v2 apps. Rust crate + TypeScript bindings.

## Structure

```
├── Cargo.toml            # Rust crate: sol-p4-tools
├── src/
│   ├── p4.rs             # P4 commands, connection, stale/concurrent checks
│   ├── workspace.rs      # .p4config detection, project root resolution
│   ├── metadata_io.rs    # BOM-aware JSON I/O, partition dir loader, atomic write
│   └── partition.rs      # SHA1-based 16-way partition postfix
├── ts/
│   ├── package.json      # @alt9github/sol-p4-tools
│   └── src/
│       └── p4-client.ts  # Tauri invoke wrappers + error categorization (KO)
└── example/              # Minimal Tauri demo app
```

## Usage

### Rust
```toml
[dependencies]
sol-p4-tools = { git = "https://github.com/alt9github/sol-p4-tools", tag = "v0.1.0" }
```

### TypeScript
```jsonc
{ "dependencies": { "@alt9github/sol-p4-tools": "github:alt9github/sol-p4-tools#v0.1.0" } }
```

### Example App
```bash
cd example
npm install
npm run tauri dev
```

## License

MIT

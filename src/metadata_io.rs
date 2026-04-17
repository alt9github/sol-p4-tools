use std::path::{Path, PathBuf};

pub fn strip_bom(bytes: &[u8]) -> &str {
    let s = std::str::from_utf8(bytes).unwrap_or("");
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

pub fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let text = strip_bom(&bytes);
    serde_json::from_str(text).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn load_metadata_dir(data_dir: &str, sub: &str) -> Vec<serde_json::Value> {
    let dir = PathBuf::from(data_dir).join(sub);
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut items = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") { continue; }
        let bytes = match std::fs::read(entry.path()) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let text = strip_bom(&bytes);
        let parsed: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let serde_json::Value::Array(arr) = parsed {
            for mut item in arr {
                if let serde_json::Value::Object(ref mut map) = item {
                    map.insert("_sourceFile".to_string(), serde_json::Value::String(name.clone()));
                }
                items.push(item);
            }
        }
    }
    items
}

pub fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {e}"))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &content).map_err(|e| format!("write tmp: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

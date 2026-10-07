//! Read only the glTF JSON chunk; embedded textures and meshes are not needed for avatar scale.
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
pub fn read(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).map_err(|e| e.to_string())?;
    let bytes = if &magic == b"glTF" {
        let mut header = [0u8; 16];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        let version = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let total = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64;
        let len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        if version != 2
            || total != size
            || &header[12..16] != b"JSON"
            || len > 16 * 1024 * 1024
            || len as u64 + 20 > size
        {
            return Err("Invalid glTF 2.0 JSON chunk".into());
        }
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        bytes
    } else {
        if size > 16 * 1024 * 1024 {
            return Err("Avatar JSON exceeds 16 MiB".into());
        }
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        bytes
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if value["asset"]["version"] != "2.0" || !value["nodes"].is_array() {
        return Err("Select a VRM or glTF 2.0 avatar".into());
    }
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

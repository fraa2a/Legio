pub(crate) fn encode(metadata: Vec<u8>, bytes: Vec<u8>) -> Result<tauri::ipc::Response, String> {
    if metadata.len() > 4096 {
        return Err("Artwork metadata exceeds the size limit".to_owned());
    }
    let mut payload = Vec::with_capacity(4 + metadata.len() + bytes.len());
    payload.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    payload.extend_from_slice(&metadata);
    payload.extend_from_slice(&bytes);
    Ok(tauri::ipc::Response::new(payload))
}

pub(crate) fn encode(metadata: Vec<u8>, bytes: Vec<u8>) -> Result<tauri::ipc::Response, String> {
    if metadata.len() > 4096 {
        return Err("Artwork metadata exceeds the size limit".to_owned());
    }
    crate::application_log::checkpoint(
        "artwork_ipc",
        format!(
            "stage=payload_start; image_bytes={}; metadata_bytes={}",
            bytes.len(),
            metadata.len()
        ),
    );
    let mut payload = Vec::with_capacity(4 + metadata.len() + bytes.len());
    payload.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    payload.extend_from_slice(&metadata);
    payload.extend_from_slice(&bytes);
    crate::application_log::checkpoint(
        "artwork_ipc",
        format!("stage=payload_ready; bytes={}", payload.len()),
    );
    Ok(tauri::ipc::Response::new(payload))
}

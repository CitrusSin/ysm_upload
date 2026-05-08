use std::path::Path;

use anyhow::{Result, ensure};
use tokio::fs;

use crate::config::LocalStorageConfig;

use super::UploadResult;

pub async fn upload_via_local(
    local: &LocalStorageConfig,
    file_name: &str,
    file_bytes: &[u8],
) -> Result<UploadResult> {
    let upload_dir = local.upload_dir.trim();
    ensure!(!upload_dir.is_empty(), "Local upload_dir is empty");

    let upload_dir_path = Path::new(upload_dir);
    fs::create_dir_all(upload_dir_path)
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to create local upload directory {upload_dir}"))?;

    let target_path = upload_dir_path.join(file_name);
    fs::write(&target_path, file_bytes)
        .await
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to write uploaded model to {}", target_path.display()))?;

    Ok(UploadResult {
        stored_file_name: file_name.to_string(),
        upload_dir: upload_dir.to_string(),
    })
}

use anyhow::Context;
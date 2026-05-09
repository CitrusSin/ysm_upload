use std::borrow::Cow;

use anyhow::Result;

use crate::config::MCSManagerConfig;
use crate::external_api::mcsmanager::MCSManagerClient;

use super::UploadResult;

pub async fn upload_via_mcsmanager(
    mcsm: &MCSManagerConfig,
    file_name: &str,
    file_bytes: impl Into<Cow<'static, [u8]>>,
) -> Result<UploadResult> {
    let client = MCSManagerClient::new(mcsm)?;
    client
        .upload_file(&mcsm.upload_dir, file_name, file_bytes)
        .await?;

    Ok(UploadResult {
        stored_file_name: file_name.to_string(),
        upload_dir: mcsm.upload_dir.clone(),
    })
}

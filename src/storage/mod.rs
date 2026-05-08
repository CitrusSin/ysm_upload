mod local;
mod mcsmanager;
mod sftp;

use std::borrow::Cow;

use anyhow::{Context, Result};

use crate::config::{
    Config,
    YsmStorageBackend,
};

pub struct UploadResult {
    pub stored_file_name: String,
    pub upload_dir: String,
}

pub async fn upload_model(
    config: &Config,
    file_name: &str,
    file_bytes: impl Into<Cow<'static, [u8]>>,
) -> Result<UploadResult> {
    match config.ysm_storage.backend {
        YsmStorageBackend::MCSManager => mcsmanager::upload_via_mcsmanager(&config.mcsmanager, file_name, file_bytes).await,
        YsmStorageBackend::LocalFile => local::upload_via_local(
            config
                .ysm_storage
                .local
                .as_ref()
                .context("YSM local file storage configuration is missing")?,
            file_name,
            file_bytes.into().as_ref(),
        )
        .await,
        YsmStorageBackend::Sftp => sftp::upload_via_sftp(
            config
                .ysm_storage
                .sftp
                .as_ref()
                .context("YSM SFTP storage configuration is missing")?,
            file_name,
            file_bytes.into().as_ref(),
        )
        .await
    }
}

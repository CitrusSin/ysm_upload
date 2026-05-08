use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use anyhow::{Context, Result};
use rusftp::{client::SftpClient, message::PFlags};
use russh::client::{self, Config as SshConfig};
use russh_keys::key::PublicKey;
use tokio::io::AsyncWriteExt;

use crate::config::SftpStorageConfig;

use super::UploadResult;

pub async fn upload_via_sftp(
    sftp: &SftpStorageConfig,
    file_name: &str,
    file_bytes: &[u8],
) -> Result<UploadResult> {
    let sftp_client = connect_sftp_client(
        SocketAddr::new(sftp.host.parse()?, sftp.port),
        sftp.username.trim(),
        sftp.password.trim(),
    ).await?;
    upload_data(
        &sftp_client,
        sftp.remote_dir.trim(),
        file_name,
        file_bytes,
    )
    .await
}

async fn upload_data(
    sftp: &SftpClient,
    remote_dir: &str,
    file_name: &str,
    file_bytes: &[u8],
) -> Result<UploadResult> {
    let remote_path = build_remote_path(remote_dir, file_name);

    ensure_remote_directory_tree(&sftp, remote_dir).await?;

    let mut remote_file = sftp
        .open_with_flags(&remote_path, PFlags::CREATE | PFlags::TRUNCATE | PFlags::WRITE)
        .await
        .with_context(|| format!("Failed to create remote file {remote_path}"))?;
    remote_file
        .write_all(file_bytes.as_ref())
        .await
        .with_context(|| format!("Failed to write remote file {remote_path}"))?;
    remote_file
        .flush()
        .await
        .with_context(|| format!("Failed to flush remote file {remote_path}"))?;
    remote_file
        .close()
        .await
        .with_context(|| format!("Failed to close remote file {remote_path}"))?;

    Ok(UploadResult {
        stored_file_name: file_name.to_string(),
        upload_dir: remote_dir.to_string(),
    })
}

async fn connect_sftp_client(
    address: std::net::SocketAddr,
    username: &str,
    password: &str,
) -> Result<SftpClient> {
    let config = Arc::new(SshConfig::default());
    let mut ssh = client::connect(config, address, NoCheckHandler)
        .await
        .with_context(|| format!("Failed to connect to SSH server {address}"))?;
    ssh
        .authenticate_password(username, password)
        .await
        .with_context(|| format!("Failed password authentication for {username}@{address}"))?;

    SftpClient::new(ssh)
        .await
        .with_context(|| format!("Failed to initialize SFTP session for {username}@{address}"))
}

async fn ensure_remote_directory_tree(
    sftp: &SftpClient,
    remote_dir: &str,
) -> Result<()> {
    for directory in parent_directories(remote_dir) {
        if sftp.stat(directory.as_str()).await.is_ok() {
            continue;
        }

        sftp
            .mkdir(directory.as_str())
            .await
            .with_context(|| format!("Failed to create remote directory {directory}"))?;
    }

    Ok(())
}

struct NoCheckHandler;

#[async_trait]
impl russh::client::Handler for NoCheckHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

fn build_remote_path(remote_dir: &str, file_name: &str) -> String {
    let trimmed = remote_dir.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        file_name.to_string()
    } else {
        format!("{trimmed}/{file_name}")
    }
}

fn parent_directories(path: &str) -> Vec<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let has_root = trimmed.starts_with('/');
    let mut current = String::new();
    let mut directories = Vec::new();

    for segment in trimmed.split('/').filter(|segment| !segment.is_empty()) {
        if has_root && current.is_empty() {
            current.push('/');
            current.push_str(segment);
        } else if current.is_empty() {
            current.push_str(segment);
        } else {
            current.push('/');
            current.push_str(segment);
        }
        directories.push(current.clone());
    }

    directories
}

#[cfg(test)]
mod tests {

    use super::{build_remote_path, parent_directories};

    #[test]
    fn parent_directories_builds_absolute_path_chain() {
        assert_eq!(
            parent_directories("/config/yes_steve_model/auth"),
            vec![
                "/config".to_string(),
                "/config/yes_steve_model".to_string(),
                "/config/yes_steve_model/auth".to_string(),
            ]
        );
    }

    #[test]
    fn build_remote_path_joins_directory_and_file() {
        assert_eq!(build_remote_path("/config/auth/", "model.ysm"), "/config/auth/model.ysm");
        assert_eq!(build_remote_path("", "model.ysm"), "model.ysm");
    }
}
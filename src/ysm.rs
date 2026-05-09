use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Json,
    extract::{Multipart, State},
};
use serde_json::json;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    AppResult, AppState,
    config::{Config, YsmCommandBackend},
    external_api::mcsmanager::MCSManagerClient,
    oauth::UnifiedUserInfo,
    rcon::RconSession,
};
use crate::storage;

pub async fn upload_authorized_model(
    State(state): State<Arc<AppState>>,
    user: UnifiedUserInfo,
    mut multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let upload = parse_upload_request(&mut multipart).await?;

    let owned_profile = user
        .profiles
        .iter()
        .find(|profile| profile.id == upload.profile_uuid)
        .context("Rejected YSM upload for non-owned profile UUID")?;
    let profile_name = owned_profile.name.clone();

    let stored_file_name = format!("{}.{}", upload.model_id, upload.extension);
    let upload_result = storage::upload_model(
        &state.config,
        &stored_file_name,
        upload.file_bytes,
    )
    .await?;

    let mut command_executor = CommandExecutor::connect(&state.config).await?;

    let reload_command = "ysm model reload";
    let reload_result = command_executor
        .exec_command(reload_command)
        .await
        .context("Failed to reload YSM models")?;
    debug!("Reload result: {}", reload_result);

    tokio::time::sleep(state.config.reload_delay).await;

    let authorize_command = format!(
        "ysm auth {} add {}",
        &profile_name,
        &stored_file_name
    );
    let authorize_result = command_executor
        .exec_command(&authorize_command)
        .await
        .context("Failed to authorize YSM model")?;

    debug!("Authorize result: {}", authorize_result);

    info!(
        profile = %profile_name,
        uuid = %upload.profile_uuid,
        model_id = %upload.model_id,
        file_name = %stored_file_name,
        "Uploaded YSM model and granted authorization"
    );

    Ok(Json(json!({
        "success": true,
        "profile_name": profile_name,
        "model_id": upload.model_id,
        "stored_file_name": upload_result.stored_file_name,
        "upload_dir": upload_result.upload_dir,
        "reload_command": reload_command,
        "authorize_command": authorize_command,
        "reload_result": reload_result,
        "authorize_result": authorize_result,
    })))
}

enum CommandExecutor {
    Rcon(RconSession),
    MCSManager {
        client: MCSManagerClient,
        output_log_size_kb: u16,
        command_wait: std::time::Duration,
    },
}

impl CommandExecutor {
    async fn connect(config: &Config) -> Result<Self> {
        match config.ysm_command.backend {
            YsmCommandBackend::Rcon => {
                let session = RconSession::connect(
                    (config.rcon.host.as_str(), config.rcon.port),
                    &config.rcon.password,
                )
                .await
                .context("Failed to connect to RCON")?;
                Ok(Self::Rcon(session))
            }
            YsmCommandBackend::MCSManager => Ok(Self::MCSManager {
                client: MCSManagerClient::new(&config.mcsmanager)
                    .context("Failed to create MCSManager API client")?,
                output_log_size_kb: config.ysm_command.mcsm_output_log_size_kb,
                command_wait: config.ysm_command.mcsm_command_wait,
            }),
        }
    }

    async fn exec_command(&mut self, command: &str) -> Result<String> {
        match self {
            Self::Rcon(session) => session
                .exec_command(command)
                .await
                .context("Failed to execute command through RCON"),
            Self::MCSManager {
                client,
                output_log_size_kb,
                command_wait,
            } => {
                let before_log = client
                    .get_output_log(Some(*output_log_size_kb))
                    .await
                    .context("Failed to fetch MCSManager output log before command")?;
                let instance_uuid = client
                    .send_command(command)
                    .await
                    .context("Failed to execute command through MCSManager API")?;
                tokio::time::sleep(*command_wait).await;
                let after_log = client
                    .get_output_log(Some(*output_log_size_kb))
                    .await
                    .context("Failed to fetch MCSManager output log after command")?;
                Ok(format_mcsmanager_command_result(
                    &instance_uuid,
                    extract_new_output(&before_log, &after_log),
                ))
            }
        }
    }
}

fn extract_new_output(before_log: &str, after_log: &str) -> String {
    if before_log.is_empty() {
        return after_log.trim().to_string();
    }

    if let Some(stripped) = after_log.strip_prefix(before_log) {
        return stripped.trim().to_string();
    }

    match longest_suffix_prefix_overlap(before_log, after_log) {
        Some(overlap_len) if overlap_len < after_log.len() => after_log[overlap_len..].trim().to_string(),
        Some(_) => String::new(),
        None => after_log.trim().to_string(),
    }
}

fn longest_suffix_prefix_overlap(before_log: &str, after_log: &str) -> Option<usize> {
    let max_overlap = before_log.len().min(after_log.len());
    (1..=max_overlap)
        .rev()
        .find(|&len| {
            if !after_log.is_char_boundary(len) {
                return false;
            }

            let start = before_log.len() - len;
            if !before_log.is_char_boundary(start) {
                return false;
            }

            before_log.as_bytes()[start..] == after_log.as_bytes()[..len]
        })
}

fn format_mcsmanager_command_result(instance_uuid: &str, output: String) -> String {
    if output.is_empty() {
        return format!(
            "Command accepted by MCSManager for instance {instance_uuid}, but no new output was captured"
        );
    }

    format!(
        "Command accepted by MCSManager for instance {instance_uuid}\n{output}"
    )
}

struct ParsedUploadRequest {
    model_id: String,
    extension: String,
    file_bytes: Vec<u8>,
    profile_uuid: Uuid,
}

async fn parse_upload_request(
    multipart: &mut Multipart,
) -> Result<ParsedUploadRequest> {
    let mut file_name = None;
    let mut file_bytes = None;
    let mut profile_uuid: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .context("Failed to read multipart field")?
    {
        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "profile_uuid" => {
                let value = field
                    .text()
                    .await
                    .context("Invalid profile_uuid field")?;
                profile_uuid = Some(
                    Uuid::parse_str(&value)?
                )
            }
            "file" => {
                file_name = field.file_name().map(|value| value.to_string());
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .context("Failed to read file payload")?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let original_file_name = file_name
        .filter(|value| !value.is_empty())
        .context("Missing uploaded file")?;
    let file_bytes = file_bytes
        .filter(|value| !value.is_empty())
        .context("Uploaded file is empty")?;
    let profile_uuid = profile_uuid.context("Missing required profile_uuid")?;

    let extension = Path::new(&original_file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .context("Uploaded file is missing extension")?;
    let extension = sanitize_upload_extension(&extension)?;
    let model_id = generate_model_id(&file_bytes);

    Ok(ParsedUploadRequest {
        model_id,
        extension,
        file_bytes,
        profile_uuid,
    })
}

fn generate_model_id(file_bytes: &[u8]) -> String {
    let digest = md5::compute(file_bytes);
    bs58::encode(digest.0).into_string()
}

fn sanitize_upload_extension(value: &str) -> Result<String> {
    let trimmed = value.trim().to_ascii_lowercase();
    anyhow::ensure!(
        matches!(trimmed.as_str(), "ysm" | "zip" | "7z"),
        "unsupported upload file extension: {trimmed}"
    );
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{generate_model_id, sanitize_upload_extension};

    #[test]
    fn generated_model_id_is_stable() {
        let first = generate_model_id(b"same file bytes");
        let second = generate_model_id(b"same file bytes");
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn reject_unsupported_upload_extension() {
        let error = sanitize_upload_extension("exe").expect_err("extension should be rejected");
        assert!(error.to_string().contains("unsupported upload file extension"));
    }

    #[test]
    fn generated_model_id_changes_with_file_content() {
        let first = generate_model_id(b"file A");
        let second = generate_model_id(b"file B");
        assert_ne!(first, second);
    }
}
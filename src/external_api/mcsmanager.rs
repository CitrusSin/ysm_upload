use std::borrow::Cow;

use anyhow::{Context, Result, ensure};
use reqwest::{Client, Url, multipart};
use serde::Deserialize;

use crate::config::MCSManagerConfig;

pub struct MCSManagerClient {
    http: Client,
    base_url: Url,
    api_key: String,
    daemon_id: String,
    instance_id: String,
}

impl MCSManagerClient {
    pub fn new(config: &MCSManagerConfig) -> Result<Self> {
        config.validate_api_access()?;

        Ok(Self {
            http: Client::new(),
            base_url: Url::parse(config.base_url.trim_end_matches('/'))
                .context("Invalid MCSManager base URL")?,
            api_key: config.api_key.clone(),
            daemon_id: config.daemon_id.clone(),
            instance_id: config.instance_id.clone(),
        })
    }

    pub async fn upload_file(
        &self,
        upload_dir: &str,
        file_name: &str,
        file_bytes: impl Into<Cow<'static, [u8]>>,
    ) -> Result<()> {
        let upload_ticket = self.request_upload_ticket(upload_dir).await?;
        self.upload_file_to_daemon(&upload_ticket, file_name, file_bytes)
            .await
    }

    pub async fn send_command(&self, command: &str) -> Result<String> {
        let response = self
            .http
            .get(self.endpoint_url("/api/protected_instance/command")?)
            .query(&[
                ("apikey", self.api_key.as_str()),
                ("daemonId", self.daemon_id.as_str()),
                ("uuid", self.instance_id.as_str()),
                ("command", command),
            ])
            .header("X-Requested-With", "XMLHttpRequest")
            .send()
            .await
            .context("Failed to send command via MCSManager")?;

        let api_response: MCSManagerApiResponse<InstanceCommandResponse> = response
            .json()
            .await
            .context("Failed to parse MCSManager command response")?;

        ensure!(
            api_response.status == 200,
            "MCSManager refused command request: status={}",
            api_response.status
        );

        Ok(api_response.data.instance_uuid)
    }

    pub async fn get_output_log(&self, size_kb: Option<u16>) -> Result<String> {
        let mut request = self
            .http
            .get(self.endpoint_url("/api/protected_instance/outputlog")?)
            .query(&[
                ("apikey", self.api_key.as_str()),
                ("daemonId", self.daemon_id.as_str()),
                ("uuid", self.instance_id.as_str()),
            ])
            .header("X-Requested-With", "XMLHttpRequest");

        if let Some(size_kb) = size_kb {
            request = request.query(&[("size", size_kb)]);
        }

        let response = request
            .send()
            .await
            .context("Failed to fetch MCSManager output log")?;

        let api_response: MCSManagerApiResponse<String> = response
            .json()
            .await
            .context("Failed to parse MCSManager output log response")?;

        ensure!(
            api_response.status == 200,
            "MCSManager refused output log request: status={}",
            api_response.status
        );

        Ok(api_response.data)
    }

    async fn request_upload_ticket(&self, upload_dir: &str) -> Result<UploadTicket> {
        let response = self
            .http
            .post(self.endpoint_url("/api/files/upload")?)
            .query(&[
                ("apikey", self.api_key.as_str()),
                ("upload_dir", upload_dir),
                ("daemonId", self.daemon_id.as_str()),
                ("uuid", self.instance_id.as_str()),
            ])
            .header("X-Requested-With", "XMLHttpRequest")
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("Failed to request MCSManager upload ticket")?;

        let api_response: MCSManagerApiResponse<UploadTicket> = response
            .json()
            .await
            .context("Failed to parse MCSManager upload ticket response")?;

        ensure!(
            api_response.status == 200,
            "MCSManager refused upload ticket request: status={}",
            api_response.status
        );

        Ok(api_response.data)
    }

    async fn upload_file_to_daemon(
        &self,
        ticket: &UploadTicket,
        file_name: &str,
        file_bytes: impl Into<Cow<'static, [u8]>>,
    ) -> Result<()> {
        let content_type = mime_guess::from_path(file_name)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string();
        let form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(file_bytes.into())
                .file_name(file_name.to_string())
                .mime_str(&content_type)
                .context("Failed to build multipart upload body")?,
        );

        let upload_url = build_daemon_upload_url_from_ticket(&self.base_url, ticket)?;
        let response = self
            .http
            .post(upload_url)
            .multipart(form)
            .send()
            .await
            .context("Failed to upload model file to daemon")?;

        ensure!(
            response.status().is_success(),
            "Daemon upload request returned non-success status: status={}",
            response.status()
        );

        Ok(())
    }

    fn endpoint_url(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .with_context(|| format!("Failed to build MCSManager API URL for path {path}"))
    }
}

#[derive(Deserialize)]
struct MCSManagerApiResponse<T> {
    status: u16,
    data: T,
}

#[derive(Deserialize)]
struct InstanceCommandResponse {
    #[serde(rename = "instanceUuid")]
    instance_uuid: String,
}

#[derive(Deserialize)]
struct UploadTicket {
    password: String,
    addr: String,
    #[serde(default, rename = "remoteMappings")]
    remote_mappings: Vec<RemoteMapping>,
}

#[derive(Deserialize)]
struct RemoteMapping {
    from: RemoteEndpoint,
    to: RemoteEndpoint,
}

#[derive(Deserialize)]
struct RemoteEndpoint {
    addr: String,
    #[serde(default)]
    prefix: String,
}

fn build_daemon_upload_url_from_ticket(base_url: &Url, ticket: &UploadTicket) -> Result<String> {
    let authority = url_authority(base_url)?;

    let (daemon_addr, daemon_prefix) = ticket
        .remote_mappings
        .iter()
        .find(|mapping| mapping.from.addr.eq_ignore_ascii_case(&authority))
        .map(|mapping| (mapping.to.addr.as_str(), mapping.to.prefix.as_str()))
        .unwrap_or((ticket.addr.as_str(), ""));

    if daemon_addr.starts_with("http://") || daemon_addr.starts_with("https://") {
        let daemon_url = Url::parse(daemon_addr).context("Invalid mapped daemon URL")?;
        return Ok(compose_upload_url(&daemon_url, daemon_prefix, &ticket.password));
    }

    let mut daemon_url = base_url.clone();
    daemon_url
        .set_host(Some(daemon_addr.split(':').next().unwrap_or_default()))
        .map_err(|_| anyhow::anyhow!("Invalid daemon host in upload mapping: daemon_addr={daemon_addr}"))?;
    let daemon_port = parse_port_from_addr(daemon_addr)?;
    daemon_url
        .set_port(daemon_port)
        .map_err(|_| anyhow::anyhow!("Failed to apply daemon port to upload URL: daemon_addr={daemon_addr}"))?;

    Ok(compose_upload_url(&daemon_url, daemon_prefix, &ticket.password))
}

fn compose_upload_url(base_url: &Url, prefix: &str, password: &str) -> String {
    let mut url = base_url.clone();
    let normalized_prefix = prefix.trim_matches('/');
    let path = if normalized_prefix.is_empty() {
        format!("/upload/{password}")
    } else {
        format!("/{normalized_prefix}/upload/{password}")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

fn parse_port_from_addr(addr: &str) -> Result<Option<u16>> {
    match addr.rsplit_once(':') {
        Some((_, port)) => Ok(Some(
            port.parse::<u16>()
                .with_context(|| format!("Invalid daemon port in address {addr}"))?,
        )),
        None => Ok(None),
    }
}

fn url_authority(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .with_context(|| format!("MCSManager base URL has no host: {}", url.as_str()))?;
    Ok(match url.port_or_known_default() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use anyhow::Result;


#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct YggdrasilKVPair {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct YggdrasilProfile {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub properties: Vec<YggdrasilKVPair>,
}

pub async fn get_profile_from_uuid(uuid: &Uuid) -> Result<YggdrasilProfile> {
    let client = reqwest::Client::new();
    let resp: YggdrasilProfile = client
        .get(&format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", uuid.as_simple()))
        .send().await?.error_for_status()?
        .json().await?;

    Ok(resp)
}
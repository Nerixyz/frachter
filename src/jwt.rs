use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransferClaims {
    pub role: TransferRole,
    pub id: Uuid,
    exp: i64,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
pub enum TransferRole {
    Sender,
    Receiver,
}

pub type EncodeConfig = (jsonwebtoken::EncodingKey, jsonwebtoken::Header);
pub type DecodeConfig = (jsonwebtoken::DecodingKey, jsonwebtoken::Validation);

impl TransferClaims {
    pub fn sender(id: Uuid, duration: time::Duration) -> Self {
        Self::new(TransferRole::Sender, id, duration)
    }

    pub fn new(role: TransferRole, id: Uuid, duration: time::Duration) -> Self {
        Self {
            role,
            id,
            exp: (time::OffsetDateTime::now_utc() + duration).unix_timestamp(),
        }
    }
}

pub fn encode_token(
    (key, header): &EncodeConfig,
    claims: &TransferClaims,
) -> jsonwebtoken::errors::Result<String> {
    jsonwebtoken::encode(header, claims, key)
}

pub fn decode_token(
    (key, validation): &DecodeConfig,
    token: &str,
) -> jsonwebtoken::errors::Result<TransferClaims> {
    jsonwebtoken::decode(token, key, validation).map(|t| t.claims)
}

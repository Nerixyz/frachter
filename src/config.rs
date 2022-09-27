use std::fs;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub bind: String,
    pub token: String,
    pub jwt_secret: String,
    #[serde(default)]
    pub log_filter: Option<String>,
}

pub fn parse_config() -> Config {
    let file = fs::read("config.toml")
        .expect("Couldn't open config.toml");
    toml::from_slice(&file).expect("Couldn't parse config")
}

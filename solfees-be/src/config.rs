use {serde::Deserialize, std::path::Path, tokio::fs};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Enable tracing
    #[serde(default)]
    pub tracing: ConfigTracing,
}

impl Config {
    pub async fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let contents = fs::read(path).await?;
        Ok(serde_yaml::from_slice(&contents)?)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigTracing {
    pub opentelemetry: bool,
    pub json: bool,
}

impl Default for ConfigTracing {
    fn default() -> Self {
        Self {
            opentelemetry: true,
            json: true,
        }
    }
}

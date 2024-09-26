use {
    human_size::Size,
    serde::{
        de::{self, Deserializer},
        Deserialize,
    },
    std::{
        fmt,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        str::FromStr,
        time::Duration,
    },
};

pub trait WithConfigTracing {
    fn get_tracing(&self) -> &ConfigTracing;
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigGrpc2Redis {
    pub tracing: ConfigTracing,
    pub grpc: ConfigGrpc,
    pub redis: ConfigRedisPublisher,
    pub listen_admin: ConfigListenAdmin,
}

impl WithConfigTracing for ConfigGrpc2Redis {
    fn get_tracing(&self) -> &ConfigTracing {
        &self.tracing
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigTracing {
    pub json: bool,
}

impl Default for ConfigTracing {
    fn default() -> Self {
        Self { json: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigGrpc {
    #[serde(deserialize_with = "deserialize_maybe_env")]
    pub endpoint: String,
    #[serde(deserialize_with = "deserialize_option_maybe_env")]
    pub x_token: Option<String>,
}

impl Default for ConfigGrpc {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:10000".to_owned(),
            x_token: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigRedisPublisher {
    pub endpoint: String,
    pub stream_key: String,
    pub stream_maxlen: u64,
    pub stream_field_key: String,
}

impl Default for ConfigRedisPublisher {
    fn default() -> Self {
        Self {
            endpoint: "redis://127.0.0.1:6379/".to_owned(),
            stream_key: "solfees:events".to_owned(),
            stream_maxlen: 15 * 60 * 3 * 4, // ~15min (2.5 slots per sec, 4 events per slot)
            stream_field_key: "message".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigListenAdmin {
    #[serde(deserialize_with = "deserialize_maybe_env")]
    pub bind: SocketAddr,
}

impl Default for ConfigListenAdmin {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8000),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigSolfees {
    pub tracing: ConfigTracing,
    pub redis: ConfigRedisConsumer,
    pub listen_admin: ConfigListenAdmin,
    pub listen_rpc: ConfigListenRpc,
}

impl WithConfigTracing for ConfigSolfees {
    fn get_tracing(&self) -> &ConfigTracing {
        &self.tracing
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigRedisConsumer {
    pub endpoint: String,
    pub stream_key: String,
    pub stream_field_key: String,
}

impl Default for ConfigRedisConsumer {
    fn default() -> Self {
        Self {
            endpoint: "redis://127.0.0.1:6379/".to_owned(),
            stream_key: "solfees:events".to_owned(),
            stream_field_key: "message".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ConfigListenRpc {
    #[serde(deserialize_with = "deserialize_maybe_env")]
    pub bind: SocketAddr,
    #[serde(deserialize_with = "deserialize_humansize")]
    pub body_limit: usize,
    pub request_calls_max: usize,
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,
    pub request_queue_max: usize,
    pub streams_channel_capacity: usize,
}

impl Default for ConfigListenRpc {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8000),
            body_limit: 16 * 1024,
            request_calls_max: 10,
            request_timeout: Duration::from_secs(60),
            request_queue_max: 1_000,
            streams_channel_capacity: 150,
        }
    }
}

fn deserialize_humansize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: de::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let size = Size::from_str(&value).map_err(de::Error::custom)?;
    Ok(size.to_bytes() as usize)
}

fn deserialize_maybe_env<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: serde::de::DeserializeOwned + FromStr,
    <T as FromStr>::Err: fmt::Debug,
    D: Deserializer<'de>,
{
    #[derive(Debug, PartialEq, Eq, Deserialize)]
    #[serde(untagged)]
    enum MaybeEnv<V> {
        Value(V),
        Env { env: String },
    }

    match MaybeEnv::deserialize(deserializer)? {
        MaybeEnv::Value(value) => Ok(value),
        MaybeEnv::Env { env } => std::env::var(env)
            .map_err(|error| format!("{error:?}"))
            .and_then(|value| value.parse().map_err(|error| format!("{error:?}")))
            .map_err(de::Error::custom),
    }
}

fn deserialize_option_maybe_env<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::de::DeserializeOwned + FromStr,
    <T as FromStr>::Err: fmt::Debug,
    D: Deserializer<'de>,
{
    #[derive(Debug, PartialEq, Eq, Deserialize)]
    #[serde(untagged)]
    enum MaybeEnv<V> {
        Value(Option<V>),
        Env { env: String },
    }

    match MaybeEnv::deserialize(deserializer)? {
        MaybeEnv::Value(value) => Ok(value),
        MaybeEnv::Env { env } => match std::env::var(env) {
            Ok(value) => value
                .parse()
                .map(Some)
                .map_err(|error| format!("{error:?}")),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(format!("{error:?}")),
        }
        .map_err(de::Error::custom),
    }
}

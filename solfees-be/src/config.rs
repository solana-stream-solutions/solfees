use {
    human_size::Size,
    serde::{
        de::{self, Deserializer},
        Deserialize,
    },
    std::{
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
    pub endpoint: String,
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
    #[serde(deserialize_with = "deserialize_listen")]
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
    #[serde(deserialize_with = "deserialize_listen")]
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
            body_limit: 50 * 1024,
            request_calls_max: 100,
            request_timeout: Duration::from_secs(60),
            request_queue_max: 1_000,
            streams_channel_capacity: 150,
        }
    }
}

fn deserialize_listen<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Debug, PartialEq, Eq, Hash, Deserialize)]
    #[serde(untagged)]
    enum Value {
        SocketAddr(SocketAddr),
        Port(u16),
        Env { env: String },
    }

    match Value::deserialize(deserializer)? {
        Value::SocketAddr(addr) => Ok(addr),
        Value::Port(port) => Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port)),
        Value::Env { env } => std::env::var(env)
            .map_err(|error| format!("{:}", error))
            .and_then(|value| match value.parse() {
                Ok(addr) => Ok(addr),
                Err(error) => match value.parse() {
                    Ok(port) => Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port)),
                    Err(_) => Err(format!("{:?}", error)),
                },
            })
            .map_err(de::Error::custom),
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

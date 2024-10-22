use {
    crate::version::VERSION as VERSION_INFO,
    http_body_util::{combinators::BoxBody, BodyExt, Full as FullBody},
    hyper::body::Bytes,
    prometheus::{IntCounterVec, Opts, Registry, TextEncoder},
    std::convert::Infallible,
    tracing::error,
};

lazy_static::lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    static ref VERSION: IntCounterVec = IntCounterVec::new(
        Opts::new("version", "Version info"),
        &["buildts", "git", "package", "proto", "rustc", "solana", "version"]
    ).unwrap();
}

macro_rules! register {
    ($collector:ident) => {
        REGISTRY
            .register(Box::new($collector.clone()))
            .expect("collector can't be registered");
    };
}

fn init2() {
    register!(VERSION);
    VERSION
        .with_label_values(&[
            VERSION_INFO.buildts,
            VERSION_INFO.git,
            VERSION_INFO.package,
            VERSION_INFO.proto,
            VERSION_INFO.rustc,
            VERSION_INFO.solana,
            VERSION_INFO.version,
        ])
        .inc();
}

pub fn collect_to_body() -> BoxBody<Bytes, Infallible> {
    let metrics = TextEncoder::new()
        .encode_to_string(&REGISTRY.gather())
        .unwrap_or_else(|error| {
            error!(%error, "could not encode custom metrics");
            String::new()
        });
    FullBody::new(Bytes::from(metrics)).boxed()
}

pub mod grpc2redis {
    use {
        super::{init2, REGISTRY},
        crate::grpc_geyser::CommitmentLevel,
        prometheus::{IntCounter, IntGaugeVec, Opts},
        solana_sdk::clock::Slot,
    };

    lazy_static::lazy_static! {
        static ref REDIS_SLOT_PUSHED: IntGaugeVec = IntGaugeVec::new(
            Opts::new("redis_slot_pushed", "Slot pushed to Redis by commitment"),
            &["commitment"]
        ).unwrap();

        static ref REDIS_MESSAGES_PUSHED: IntCounter = IntCounter::new("redis_messages_pushed", "Number of messages pushed to Redis stream").unwrap();
    }

    pub fn init() {
        init2();

        register!(REDIS_SLOT_PUSHED);
        register!(REDIS_MESSAGES_PUSHED);
    }

    pub fn redis_slot_pushed_set(commitment: CommitmentLevel, slot: Slot) {
        REDIS_SLOT_PUSHED
            .with_label_values(&[commitment.as_str()])
            .set(slot as i64);
    }

    pub fn redis_messages_pushed_inc_by(delta: usize) {
        REDIS_MESSAGES_PUSHED.inc_by(delta as u64);
    }
}

pub mod solfees_be {
    use {
        super::{init2, REGISTRY},
        crate::{
            grpc_geyser::CommitmentLevel,
            rpc_solana::{RpcRequestType, SolanaRpcMode},
        },
        http::StatusCode,
        prometheus::{HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Opts},
        solana_sdk::clock::Slot,
        std::{borrow::Cow, time::Duration},
    };

    // TODO: replace with std
    lazy_static::lazy_static! {
        static ref LATEST_SLOT: IntGaugeVec = IntGaugeVec::new(
            Opts::new("latest_slot", "Latest slot received from Redis by commitment"),
            &["commitment"]
        ).unwrap();

        // TOOD: name
        // # HELP requests_duration_seconds Elapsed time per request
        // # TYPE requests_duration_seconds histogram
        // requests_duration_seconds_bucket{api="frontend",status="200",le="0.005"} 0
        static ref REQUESTS_DURATION_SECONDS: HistogramVec = HistogramVec::new(
            HistogramOpts {
                common_opts: Opts::new("requests_duration_seconds", "Elapsed time per request"),
                buckets: vec![
                    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
                ]
            },
            &["api", "status"]
        ).unwrap();

        static ref REQUESTS_CALLS_TOTAL: IntCounterVec = IntCounterVec::new(
            Opts::new("requests_calls_total", "Total number of request calls by API and method"),
            &["api", "method"]
        ).unwrap();

        static ref REQUESTS_QUEUE_SIZE: IntGauge = IntGauge::new(
            "requests_queue_size", "Queue size by API and method"
        ).unwrap();

        static ref WEBSOCKETS_ALIVE_TOTAL: IntGaugeVec = IntGaugeVec::new(
            Opts::new("websockets_alive_total", "Total number of alive WebSocket connections"),
            &["api"]
        ).unwrap();
    }

    pub fn init() {
        init2();

        register!(LATEST_SLOT);
        register!(REQUESTS_DURATION_SECONDS);
        register!(REQUESTS_CALLS_TOTAL);
        register!(REQUESTS_QUEUE_SIZE);
        register!(WEBSOCKETS_ALIVE_TOTAL);
    }

    pub fn set_slot(commitment: CommitmentLevel, slot: Slot) {
        LATEST_SLOT
            .with_label_values(&[commitment.as_str()])
            .set(slot as i64);
    }

    pub fn requests_observe(api: SolanaRpcMode, status: Option<StatusCode>, duration: Duration) {
        let nanos = f64::from(duration.subsec_nanos()) / 1e9;
        let sec = duration.as_secs() as f64 + nanos;

        REQUESTS_DURATION_SECONDS
            .with_label_values(&[
                match api {
                    SolanaRpcMode::Solana => "solana",
                    SolanaRpcMode::Triton => "triton",
                    SolanaRpcMode::Solfees => "solfees",
                    SolanaRpcMode::SolfeesFrontend => "frontend",
                },
                match status.map(|s| s.as_u16()) {
                    Some(200) => Cow::Borrowed("200"),
                    Some(500) => Cow::Borrowed("500"),
                    Some(code) => Cow::Owned(code.to_string()),
                    None => Cow::Borrowed("error"),
                }
                .as_ref(),
            ])
            .observe(sec);
    }

    pub fn requests_call_inc(api: SolanaRpcMode, method: RpcRequestType) {
        REQUESTS_CALLS_TOTAL
            .with_label_values(&[
                match api {
                    SolanaRpcMode::Solana => "solana",
                    SolanaRpcMode::Triton => "triton",
                    SolanaRpcMode::Solfees => "solfees",
                    SolanaRpcMode::SolfeesFrontend => "frontend",
                },
                match method {
                    RpcRequestType::LatestBlockhash => "get_latest_blockhash",
                    RpcRequestType::LeaderSchedule => "get_leader_schedule",
                    RpcRequestType::RecentPrioritizationFees => "get_recent_prioritization_fees",
                    RpcRequestType::Slot => "get_slot",
                    RpcRequestType::Version => "get_version",
                },
            ])
            .inc()
    }

    pub fn requests_queue_size_inc() {
        REQUESTS_QUEUE_SIZE.inc();
    }

    pub fn requests_queue_size_dec() {
        REQUESTS_QUEUE_SIZE.dec();
    }

    pub fn websockets_alive_inc(api: SolanaRpcMode) {
        let api = match api {
            SolanaRpcMode::Solfees => "solfees",
            SolanaRpcMode::SolfeesFrontend => "frontend",
            _ => return,
        };
        WEBSOCKETS_ALIVE_TOTAL.with_label_values(&[api]).inc()
    }

    pub fn websockets_alive_dec(api: SolanaRpcMode) {
        let api = match api {
            SolanaRpcMode::Solfees => "solfees",
            SolanaRpcMode::SolfeesFrontend => "frontend",
            _ => return,
        };
        WEBSOCKETS_ALIVE_TOTAL.with_label_values(&[api]).dec()
    }
}

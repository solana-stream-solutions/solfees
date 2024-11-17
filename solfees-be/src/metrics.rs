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
        static ref GRPC_BLOCK_BUILD_FAILED: IntCounter = IntCounter::new(
            "grpc_block_build_failed_total", "Total number of unsuccessful block builds"
        ).unwrap();

        static ref REDIS_SLOT_PUSHED: IntGaugeVec = IntGaugeVec::new(
            Opts::new("redis_slot_pushed", "Slot pushed to Redis by commitment"),
            &["commitment"]
        ).unwrap();

        static ref REDIS_MESSAGES_PUSHED: IntCounter = IntCounter::new(
            "redis_messages_pushed_total", "Number of messages pushed to Redis stream"
        ).unwrap();
    }

    pub fn init() {
        init2();

        register!(GRPC_BLOCK_BUILD_FAILED);
        register!(REDIS_SLOT_PUSHED);
        register!(REDIS_MESSAGES_PUSHED);
    }

    pub fn grpc_block_build_failed_inc() {
        GRPC_BLOCK_BUILD_FAILED.inc();
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
            rpc_solana::{RpcRequestsStats, SolanaRpcMode},
        },
        http::{HeaderMap, StatusCode},
        prometheus::{HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Opts},
        solana_sdk::clock::Slot,
        std::{
            borrow::Cow,
            sync::Arc,
            time::{Duration, Instant},
        },
    };

    lazy_static::lazy_static! {
        static ref LATEST_SLOT: IntGaugeVec = IntGaugeVec::new(
            Opts::new("latest_slot", "Latest slot received from Redis by commitment"),
            &["commitment"]
        ).unwrap();

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

        static ref CLIENT_USAGE_CPU_TOTAL: IntCounterVec = IntCounterVec::new(
            Opts::new("client_usage_cpu_total", "Total number of CPU usage in nanoseconds"),
            &["client_id", "subscription_id"]
        ).unwrap();

        static ref CLIENT_USAGE_EGRESS_WS_TOTAL: IntCounterVec = IntCounterVec::new(
            Opts::new("client_usage_egress_total", "Total number of bytes sent over WebSocket"),
            &["client_id", "subscription_id"]
        ).unwrap();
    }

    pub fn init() {
        init2();

        register!(LATEST_SLOT);
        register!(REQUESTS_DURATION_SECONDS);
        register!(REQUESTS_CALLS_TOTAL);
        register!(REQUESTS_QUEUE_SIZE);
        register!(WEBSOCKETS_ALIVE_TOTAL);
        register!(CLIENT_USAGE_CPU_TOTAL);
        register!(CLIENT_USAGE_EGRESS_WS_TOTAL);
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
                api.as_str(),
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

    pub fn requests_call_inc(api: SolanaRpcMode, stats: RpcRequestsStats) {
        REQUESTS_CALLS_TOTAL
            .with_label_values(&[api.as_str(), "get_latest_blockhash"])
            .inc_by(stats.latest_blockhash);
        REQUESTS_CALLS_TOTAL
            .with_label_values(&[api.as_str(), "get_leader_schedule"])
            .inc_by(stats.leader_schedule);
        REQUESTS_CALLS_TOTAL
            .with_label_values(&[api.as_str(), "get_recent_prioritization_fees"])
            .inc_by(stats.recent_prioritization_fees);
        REQUESTS_CALLS_TOTAL
            .with_label_values(&[api.as_str(), "get_slot"])
            .inc_by(stats.slot);
        REQUESTS_CALLS_TOTAL
            .with_label_values(&[api.as_str(), "get_version"])
            .inc_by(stats.version);
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

    #[derive(Debug)]
    struct ClientIdInner {
        client_id: String,
        subsription_id: String,
    }

    #[derive(Debug, Clone)]
    pub struct ClientId {
        inner: Arc<ClientIdInner>,
    }

    impl ClientId {
        pub fn new(headers: &HeaderMap) -> Self {
            Self {
                inner: Arc::new(ClientIdInner {
                    client_id: Self::get(headers, "x-client-id"),
                    subsription_id: Self::get(headers, "x-subscription-id"),
                }),
            }
        }

        fn get(headers: &HeaderMap, key: &str) -> String {
            headers
                .get(key)
                .and_then(|id| id.to_str().ok())
                .map(String::from)
                .unwrap_or_default()
        }

        pub fn start_timer(&self) -> ClientIdTimer {
            ClientIdTimer::new(self)
        }

        pub fn observe_cpu(&self, nanos: u64) {
            CLIENT_USAGE_CPU_TOTAL
                .with_label_values(&[&self.inner.client_id, &self.inner.subsription_id])
                .inc_by(nanos);
        }

        pub fn observe_egress_ws(&self, bytes: u64) {
            CLIENT_USAGE_EGRESS_WS_TOTAL
                .with_label_values(&[&self.inner.client_id, &self.inner.subsription_id])
                .inc_by(bytes);
        }
    }

    #[derive(Debug)]
    pub struct ClientIdTimer<'a> {
        client_id: &'a ClientId,
        start: Instant,
        observed: bool,
    }

    impl<'a> Drop for ClientIdTimer<'a> {
        fn drop(&mut self) {
            if !self.observed {
                self.observe(true);
            }
        }
    }

    impl<'a> ClientIdTimer<'a> {
        fn new(client_id: &'a ClientId) -> Self {
            Self {
                client_id,
                start: Instant::now(),
                observed: false,
            }
        }

        fn observe(&mut self, record: bool) {
            self.observed = true;
            if record {
                let elapsed = Instant::now().saturating_duration_since(self.start);
                let nanos = elapsed.as_secs() * 1_000_000_000 + elapsed.subsec_nanos() as u64;
                self.client_id.observe_cpu(nanos);
            }
        }

        pub fn stop_and_record(mut self) {
            self.observe(true);
        }

        pub fn stop_and_discard(mut self) {
            self.observe(false);
        }
    }
}

use {
    crate::{grpc_geyser::CommitmentLevel, version::VERSION as VERSION_INFO},
    http_body_util::{combinators::BoxBody, BodyExt, Full as FullBody},
    hyper::body::Bytes,
    prometheus::{IntCounter, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder},
    solana_sdk::clock::Slot,
    std::convert::Infallible,
    tracing::error,
};

lazy_static::lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    static ref VERSION: IntCounterVec = IntCounterVec::new(
        Opts::new("version", "Version info"),
        &["buildts", "git", "package", "proto", "rustc", "solana", "version"]
    ).unwrap();

    static ref GRPC_GEYSER_SLOT: IntGaugeVec = IntGaugeVec::new(
        Opts::new("grpc_geyser_received", "gRPC geyser slot by commitment"),
        &["commitment"]
    ).unwrap();

    static ref REDIS_MESSAGES_PUSHED: IntCounter = IntCounter::new("redis_messages_pushed", "Number of messages pushed to Redis stream").unwrap();
}

pub fn register_custom_metrics() {
    macro_rules! register {
        ($collector:ident) => {
            REGISTRY
                .register(Box::new($collector.clone()))
                .expect("collector can't be registered");
        };
    }

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

    register!(GRPC_GEYSER_SLOT);
    register!(REDIS_MESSAGES_PUSHED);
}

pub fn create_response() -> BoxBody<Bytes, Infallible> {
    let metrics = REGISTRY.gather();
    let metrics = TextEncoder::new()
        .encode_to_string(&metrics)
        .unwrap_or_else(|error| {
            error!(%error, "could not encode custom metrics");
            String::new()
        });
    FullBody::new(Bytes::from(metrics)).boxed()
}

pub fn grpc_geyser_slot_set(commitment: CommitmentLevel, slot: Slot) {
    GRPC_GEYSER_SLOT
        .with_label_values(&[commitment.as_str()])
        .set(slot as i64);
}

pub fn redis_messages_pushed_inc_by(delta: usize) {
    REDIS_MESSAGES_PUSHED.inc_by(delta as u64);
}

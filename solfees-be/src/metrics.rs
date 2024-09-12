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
    use super::init2;

    pub fn init() {
        init2();
    }

    //
}

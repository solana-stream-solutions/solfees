use {
    anyhow::Context,
    clap::Parser,
    redis::{AsyncConnectionConfig, Client},
    solfees_be::{config::ConfigGrpc2Redis as Config, grpc_geyser::GeyserMessage, metrics},
    std::{
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::{signal::unix::SignalKind, sync::Notify},
    tracing::{error, warn},
};

fn main() -> anyhow::Result<()> {
    metrics::register_custom_metrics();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name_fn(|| {
            static THREAD_ID: AtomicU64 = AtomicU64::new(0);
            let id = THREAD_ID.fetch_add(1, Ordering::Relaxed);
            format!("grpc2redis-{id:02}")
        })
        .build()?
        .block_on(main2())
}

async fn main2() -> anyhow::Result<()> {
    let args = solfees_be::cli::Args::parse();
    let config = args.load_config::<Config>().await?;
    if args.check {
        return Ok(());
    }

    solfees_be::tracing::init(config.tracing.json)?;

    let rpc_admin_shutdown = Arc::new(Notify::new());
    let rpc_admin_fut = tokio::spawn(solfees_be::rpc::run_admin_server(
        config.listen_admin.bind,
        Arc::clone(&rpc_admin_shutdown),
    ));

    let client =
        Client::open(config.redis.endpoint.clone()).context("failed to create Redis client")?;
    let mut connection = client
        .get_multiplexed_async_connection_with_config(
            &AsyncConnectionConfig::new().set_connection_timeout(Duration::from_secs(2)),
        )
        .await
        .context("failed to get Redis connection")?;

    let mut geyser = solfees_be::grpc_geyser::subscribe(config.grpc.endpoint, config.grpc.x_token)
        .await
        .context("failed to open gRPC subscription")?;

    let mut shutdown_rx = solfees_be::cli::shutdown_signal();
    let sigint = SignalKind::interrupt();
    let sigterm = SignalKind::terminate();

    loop {
        let mut pipe = redis::pipe();

        let mut messages = tokio::select! {
            signal = shutdown_rx.recv() => {
                match signal {
                    Some(signal) if signal == sigint => warn!("SIGINT received, exit..."),
                    Some(signal) if signal == sigterm => warn!("SIGTERM received, exit..."),
                    Some(signal) => warn!("unknown signal received ({signal:?}), exit..."),
                    None => error!("shutdown channel is down"),
                };
                break;
            }
            value = geyser.recv() => {
                if let Some(maybe_message) = value {
                    vec![maybe_message?]
                } else {
                    error!("geyser stream finished");
                    break;
                }
            }
        };

        while let Ok(maybe_message) = geyser.try_recv() {
            messages.push(maybe_message?);
        }

        for message in messages.iter() {
            pipe.cmd("XADD")
                .arg(&config.redis.stream_key)
                .arg("MAXLEN")
                .arg("~")
                .arg(config.redis.stream_maxlen)
                .arg("*")
                .arg(&config.redis.stream_field_key)
                .arg(bincode::serialize(message).context("failed to serialize GeyserMessage")?)
                .ignore();
        }

        let _: () = pipe
            .atomic()
            .query_async(&mut connection)
            .await
            .context("failed to send data to Redis")?;

        metrics::redis_messages_pushed_inc_by(messages.len());
        for message in messages {
            if let GeyserMessage::Slot { slot, commitment } = message {
                metrics::grpc_geyser_slot_set(commitment, slot);
            }
        }
    }

    rpc_admin_shutdown.notify_one();
    tokio::select! {
        signal = shutdown_rx.recv() => {
            match signal {
                Some(signal) if signal == sigint => warn!("SIGINT received, exit..."),
                Some(signal) if signal == sigterm => warn!("SIGTERM received, exit..."),
                Some(signal) => warn!("unknown signal received ({signal:?}), exit..."),
                None => error!("shutdown channel is down"),
            };
        },
        result = rpc_admin_fut => result??,
    }

    Ok(())
}

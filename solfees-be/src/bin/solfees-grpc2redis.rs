use {
    anyhow::Context,
    futures::future::{try_join_all, FutureExt, TryFutureExt},
    redis::{AsyncConnectionConfig, Client},
    solfees_be::{
        cli,
        config::ConfigGrpc2Redis as Config,
        grpc_geyser::{self, GeyserMessage},
        metrics::grpc2redis as metrics,
        rpc_server,
    },
    std::{sync::Arc, time::Duration},
    tokio::{signal::unix::SignalKind, sync::Notify},
    tracing::{error, warn},
};

fn main() -> anyhow::Result<()> {
    cli::run_main(metrics::init, |id| format!("grpc2redis-{id:02}"), main2)
}

async fn main2(config: Config) -> anyhow::Result<()> {
    let rpc_admin_shutdown = Arc::new(Notify::new());
    let rpc_admin_fut = tokio::spawn(rpc_server::run_admin(
        config.listen_admin.bind,
        Arc::clone(&rpc_admin_shutdown),
    ))
    .map(|result| result?)
    .map_err(|error| error.context("Admin RPC failed"))
    .boxed();

    let mut spawned_tasks = try_join_all(vec![rpc_admin_fut]);

    let client =
        Client::open(config.redis.endpoint.clone()).context("failed to create Redis client")?;
    let mut connection = client
        .get_multiplexed_async_connection_with_config(
            &AsyncConnectionConfig::new().set_connection_timeout(Duration::from_secs(2)),
        )
        .await
        .context("failed to get Redis connection")?;

    let mut geyser_rx = grpc_geyser::subscribe(
        config.grpc.endpoint,
        config.grpc.x_token,
        config.rpc.endpoint,
    )
    .await
    .context("failed to open gRPC subscription")?;

    let mut shutdown_rx = cli::shutdown_signal();
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
            value = &mut spawned_tasks => {
                let _: Vec<()> = value?;
                anyhow::bail!("spawned tasks finished");
            }
            value = geyser_rx.recv() => {
                if let Some(maybe_message) = value {
                    vec![maybe_message?]
                } else {
                    error!("geyser stream finished");
                    break;
                }
            }
        };

        while let Ok(maybe_message) = geyser_rx.try_recv() {
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
            if let GeyserMessage::Status { slot, commitment } = message {
                metrics::redis_slot_pushed_set(commitment, slot);
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
        value = &mut spawned_tasks => {
            let _: Vec<()> = value?;
        }
    }

    Ok(())
}

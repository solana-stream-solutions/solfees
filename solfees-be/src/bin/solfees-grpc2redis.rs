use {
    anyhow::Context,
    futures::future::{try_join_all, FutureExt, TryFutureExt},
    redis::{AsyncConnectionConfig, Client},
    solana_sdk::clock::Epoch,
    solfees_be::{
        cli,
        config::ConfigGrpc2Redis as Config,
        grpc_geyser::{self, CommitmentLevel, GeyserMessage},
        metrics::grpc2redis as metrics,
        rpc_server,
        schedule::LeaderScheduleRpc,
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

    let saved_epochs = redis::cmd("HGETALL")
        .arg(&config.redis.epochs_key)
        .query_async::<Vec<(Epoch, Vec<u8>)>>(&mut connection)
        .await
        .context("failed to fetch epochs from Redis")?
        .into_iter()
        .map(|(epoch, data)| {
            bincode::deserialize(&data)
                .with_context(|| format!("failed to deserialie epoch {epoch}"))
                .map(|schedule| (epoch, schedule))
        })
        .collect::<Result<Vec<(Epoch, LeaderScheduleRpc)>, _>>()?;

    let (mut geyser_rx, mut schedule_rx) = grpc_geyser::subscribe(
        config.grpc.endpoint,
        config.grpc.x_token,
        config.rpc.endpoint,
        saved_epochs,
    )
    .await
    .context("failed to open gRPC subscription")?;

    let mut shutdown_rx = cli::shutdown_signal();
    let sigint = SignalKind::interrupt();
    let sigterm = SignalKind::terminate();

    let mut redis_finalized_slot = 0u64;
    loop {
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
            value = schedule_rx.recv() => {
                let Some((epoch, schedule)) = value else {
                    error!("schedule stream finished");
                    break;
                };

                let _: () = redis::pipe().cmd("HSET")
                    .arg(&config.redis.epochs_key)
                    .arg(epoch)
                    .arg(bincode::serialize(&schedule).context("failed to serialize leader schedule")?)
                    .ignore()
                    .atomic()
                    .query_async(&mut connection)
                    .await
                    .context("failed to send epoch schedule to Redis")?;

                continue;
            }
        };

        // trying to fetch all messages
        while let Ok(maybe_message) = geyser_rx.try_recv() {
            messages.push(maybe_message?);
        }

        if let Some(finalized_slot) = messages
            .iter()
            .filter_map(|message| {
                if let GeyserMessage::Status {
                    slot,
                    commitment: CommitmentLevel::Finalized,
                } = message
                {
                    Some(slot)
                } else {
                    None
                }
            })
            .max()
        {
            redis_finalized_slot = redis::cmd("EVAL")
                .arg(
                    r#"
-- redis.log(redis.LOG_WARNING, "hi");
local new = tonumber(ARGV[1])
local current = tonumber(redis.call("GET", KEYS[1]));
if current == nil or current < new then
    redis.call("SET", KEYS[1], ARGV[1]);
    return new;
else
    return current;
end
"#,
                )
                .arg(1)
                .arg(&config.redis.slot_finalized)
                .arg(finalized_slot)
                .query_async(&mut connection)
                .await
                .context("failed to get finalized slot from Redis")?;
        }

        let mut pipe_size = 0;
        let mut pipe = redis::pipe();
        for message in messages.iter() {
            if message.slot() > redis_finalized_slot.checked_sub(10).unwrap_or_default() {
                pipe_size += 1;
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
        }

        if pipe_size > 0 {
            let _: () = pipe
                .atomic()
                .query_async(&mut connection)
                .await
                .context("failed to send data to Redis stream")?;

            metrics::redis_messages_pushed_inc_by(messages.len());
            for message in messages {
                if let GeyserMessage::Status { slot, commitment } = message {
                    metrics::redis_slot_pushed_set(commitment, slot);
                }
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

use {
    futures::future::{try_join_all, FutureExt, TryFutureExt},
    solfees_be::{
        cli, config::ConfigSolfees as Config, metrics::solfees_be as metrics, redis, rpc_server,
        rpc_solana::SolanaRpc,
    },
    std::sync::Arc,
    tokio::{signal::unix::SignalKind, sync::Notify},
    tracing::{error, warn},
};

fn main() -> anyhow::Result<()> {
    cli::run_main(metrics::init, |id| format!("solfees-be-{id:02}"), main2)
}

async fn main2(config: Config) -> anyhow::Result<()> {
    let (solana_rpc, solana_rpc_futs) = SolanaRpc::new(
        config.listen_rpc.request_calls_max,
        config.listen_rpc.request_timeout,
        config.listen_rpc.request_queue_max,
        config.listen_rpc.streams_channel_capacity,
        config.listen_rpc.pool_size,
    );
    let solana_rpc_futs =
        try_join_all(solana_rpc_futs.into_iter().enumerate().map(|(index, fut)| {
            tokio::spawn(fut)
                .map(|result| result?)
                .map_err(move |error| {
                    error.context(format!("SolanaRPC update loop#{index} failed"))
                })
        }))
        .map_ok(|_vec| ())
        .boxed();

    let rpc_admin_shutdown = Arc::new(Notify::new());
    let rpc_admin_fut = tokio::spawn(rpc_server::run_admin(
        config.listen_admin.bind,
        Arc::clone(&rpc_admin_shutdown),
    ))
    .map(|result| result?)
    .map_err(|error| error.context("Admin RPC failed"))
    .boxed();

    let rpc_solfees_shutdown = Arc::new(Notify::new());
    let rpc_solfees_fut = tokio::spawn(rpc_server::run_solfees(
        config.listen_rpc.bind,
        config.listen_rpc.body_limit,
        solana_rpc.clone(),
        Arc::clone(&rpc_solfees_shutdown),
    ))
    .map(|result| result?)
    .map_err(|error| error.context("Solfees RPC failed"))
    .boxed();

    let mut spawned_tasks = try_join_all(vec![solana_rpc_futs, rpc_admin_fut, rpc_solfees_fut]);

    let mut redis_rx = redis::subscribe(config.redis).await?;

    let mut shutdown_rx = cli::shutdown_signal();
    let sigint = SignalKind::interrupt();
    let sigterm = SignalKind::terminate();

    loop {
        tokio::select! {
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
            value = redis_rx.recv() => {
                if let Some(maybe_message) = value {
                    solana_rpc.push_redis_message(maybe_message?)?;
                } else {
                    error!("redis stream finished");
                    break;
                }
            }
        }
    }

    solana_rpc.shutdown();
    rpc_admin_shutdown.notify_one();
    rpc_solfees_shutdown.notify_one();

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

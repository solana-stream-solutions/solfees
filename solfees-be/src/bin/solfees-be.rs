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
    let (solana_rpc, solana_rpc_fut) = SolanaRpc::new();
    let solana_rpc_fut = tokio::spawn(solana_rpc_fut)
        .map(|result| result?)
        .map_err(|error| error.context("SolanaRPC loop failed"))
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
        Arc::clone(&rpc_solfees_shutdown),
    ))
    .map(|result| result?)
    .map_err(|error| error.context("Solfees RPC failed"))
    .boxed();

    let mut spawned_tasks = try_join_all(vec![solana_rpc_fut, rpc_admin_fut, rpc_solfees_fut]);

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
                    let message = Arc::new(maybe_message?);
                    solana_rpc.push_geyser_message(message)?;
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

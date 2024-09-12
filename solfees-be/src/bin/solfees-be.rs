use {
    anyhow::Context,
    solfees_be::{
        cli, config::ConfigServer as Config, grpc_geyser::GeyserMessage,
        metrics::solfees_be as metrics, redis,
    },
};

fn main() -> anyhow::Result<()> {
    cli::run_main(metrics::init, |id| format!("solfees-be-{id:02}"), main2)
}

async fn main2(config: Config) -> anyhow::Result<()> {
    let mut redis_rx = redis::subscribe(config.redis).await.context("")?;
    while let Some(maybe_message) = redis_rx.recv().await {
        match maybe_message? {
            GeyserMessage::Slot { slot, commitment } => {
                tracing::info!("received {slot} with {commitment:?}");
            }
            GeyserMessage::Block {
                slot,
                hash,
                time,
                height,
                parent_slot,
                parent_hash,
                transactions,
            } => {
                tracing::info!("received block {slot}");
            }
        }
    }

    Ok(())
}

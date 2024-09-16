use {clap::Parser, solfees_be::grpc_geyser::GeyserMessage, tracing::error};

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    /// Service endpoint
    #[clap(short, long, default_value_t = String::from("http://127.0.0.1:10000"))]
    endpoint: String,

    /// Access token
    #[clap(long)]
    x_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    solfees_be::tracing::init(false)?;

    let args = Args::parse();

    let mut rx = solfees_be::grpc_geyser::subscribe(args.endpoint, args.x_token).await?;
    while let Some(message) = rx.recv().await {
        match message? {
            GeyserMessage::Status {
                slot: _,
                commitment: _,
            } => {
                //
            }
            GeyserMessage::Slot {
                slot,
                hash: _,
                time: _,
                height: _,
                parent_slot: _,
                parent_hash: _,
                transactions,
            } => {
                tracing::debug!("block meta received {slot}");

                for tx in transactions {
                    let extra_fee = tx.unit_limit as u64 * tx.unit_price;
                    let expected_fee = extra_fee / 1_000_000
                        + if extra_fee % 1_000_000 > 0 { 1 } else { 0 }
                        + 5_000 * tx.sigs_count as u64;

                    if expected_fee != tx.fee {
                        error!(
                            fee = tx.fee,
                            expected_fee,
                            sigs_count = tx.sigs_count,
                            unit_limit = tx.unit_limit,
                            unit_price = tx.unit_price,
                            units_consumed = ?tx.units_consumed,
                            "{}",
                            tx.signature
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

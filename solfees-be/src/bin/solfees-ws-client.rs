use {
    anyhow::Context,
    clap::Parser,
    futures::{future::TryFutureExt, stream::StreamExt},
    serde::Serialize,
    tokio_tungstenite::{connect_async, tungstenite::protocol::Message},
    tracing::info,
};

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(short, long, default_value_t = String::from("wss://api.solfees.io/api/solana/solfees/ws"))]
    endpoint: String,

    /// Select transactions where mentioned accounts are readWrite
    #[clap(long)]
    read_write: Option<Vec<String>>,

    /// Select transactions where mentioned accounts are readOnly
    #[clap(long)]
    read_only: Option<Vec<String>>,

    /// Up to 5 levels (bps)
    #[clap(long, default_values_t = [2000, 5000, 9000])]
    levels: Vec<u16>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionParams {
    read_write: Vec<String>,
    read_only: Vec<String>,
    levels: Vec<u16>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    solfees_be::tracing::init(false)?;

    let args = Args::parse();
    let request = serde_json::to_string(&serde_json::json!({
        "id": 0,
        "method": "SlotsSubscribe",
        "params": SubscriptionParams {
            read_write: args.read_write.unwrap_or_default(),
            read_only: args.read_only.unwrap_or_default(),
            levels: args.levels,
        }
    }))
    .context("failed to create request")?;

    let (ws_stream, _) = connect_async(args.endpoint)
        .await
        .context("failed to connect to WS server")?;
    let (ws_write, mut ws_read) = ws_stream.split();

    let (req_tx, req_rx) = futures::channel::mpsc::unbounded();
    req_tx.unbounded_send(Message::text(request))?;

    let req_to_ws = req_rx.map(Ok).forward(ws_write).map_err(Into::into);
    let ws_to_stdout = async move {
        loop {
            let text = match ws_read.next().await {
                Some(Ok(Message::Text(message))) => message,
                Some(Ok(Message::Binary(msg))) => String::from_utf8(msg)
                    .map_err(|_error| anyhow::anyhow!("failed to convert to string"))?,
                Some(Ok(Message::Ping(_))) => continue,
                Some(Ok(Message::Pong(_))) => continue,
                Some(Ok(Message::Frame(_))) => continue,
                Some(Ok(Message::Close(_))) => anyhow::bail!("close message received"),
                Some(Err(error)) => anyhow::bail!(error),
                None => anyhow::bail!("stream finished"),
            };
            info!("new message: {text}");
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    tokio::try_join!(req_to_ws, ws_to_stdout).map(|_| ())
}

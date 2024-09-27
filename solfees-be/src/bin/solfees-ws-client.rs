use {
    anyhow::Context,
    clap::Parser,
    futures::{future::TryFutureExt, stream::StreamExt},
    tokio_tungstenite::{connect_async, tungstenite::protocol::Message},
    tracing::info,
};

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(short, long, default_value_t = String::from("wss://api.solfees.io/api/solfees/ws"))]
    endpoint: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    solfees_be::tracing::init(false)?;

    let args = Args::parse();

    let (ws_stream, _) = connect_async(args.endpoint)
        .await
        .context("failed to connect to WS server")?;
    let (ws_write, mut ws_read) = ws_stream.split();

    let (req_tx, req_rx) = futures::channel::mpsc::unbounded();
    let text = r#"{"id":0,"method":"SlotsSubscribe","params":{"readWrite":[],"readOnly":["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"],"levels":[5000,9000]}}"#.to_owned();
    req_tx.unbounded_send(Message::text(text))?;

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
                Some(Ok(Message::Close(_))) => return Ok(()),
                Some(Err(error)) => anyhow::bail!(error),
                None => anyhow::bail!("stream is closed"),
            };
            info!("new message: {text}");
        }
    };

    tokio::try_join!(req_to_ws, ws_to_stdout).map(|_| ())
}

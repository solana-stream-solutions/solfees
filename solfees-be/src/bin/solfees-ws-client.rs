use {
    anyhow::Context,
    clap::Parser,
    futures::{future, pin_mut, StreamExt},
    tokio_tungstenite::{connect_async, tungstenite::protocol::Message},
    tracing::info,
};

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(short, long, default_value_t = String::from("ws://127.0.0.1:8000/api/solfees/ws"))]
    endpoint: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    solfees_be::tracing::init(false)?;

    let args = Args::parse();

    let (ws_stream, _) = connect_async(args.endpoint)
        .await
        .context("failed to connect to WS server")?;
    let (ws_write, ws_read) = ws_stream.split();

    let (req_tx, req_rx) = futures::channel::mpsc::unbounded();
    let text = r#"{"id": 0, "method": "SlotsSubscribe", "params": {"levels": [5000]}}"#.to_owned();
    req_tx.unbounded_send(Message::text(text))?;

    let req_to_ws = req_rx.map(Ok).forward(ws_write);
    let ws_to_stdout = {
        ws_read.for_each(|message| async move {
            info!("new message: {:?}", message);
        })
    };

    pin_mut!(req_to_ws, ws_to_stdout);
    future::select(req_to_ws, ws_to_stdout).await;

    Ok(())
}

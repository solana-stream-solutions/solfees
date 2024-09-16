use {
    crate::{
        metrics,
        rpc_solana::{SolanaRpc, SolanaRpcMode},
    },
    futures::future::TryFutureExt,
    http_body_util::{
        combinators::BoxBody, BodyExt, Empty as BodyEmpty, Full as BodyFull, Limited,
    },
    hyper::{
        body::{Bytes, Incoming as BodyIncoming},
        header::CONTENT_TYPE,
        service::service_fn,
        Request, Response, StatusCode,
    },
    hyper_tungstenite::{is_upgrade_request, tungstenite::protocol::WebSocketConfig},
    hyper_util::{
        rt::tokio::{TokioExecutor, TokioIo},
        server::{conn::auto::Builder as ServerBuilder, graceful::GracefulShutdown},
    },
    std::{convert::Infallible, net::SocketAddr, sync::Arc},
    tokio::{
        net::TcpListener,
        sync::{broadcast, Notify},
    },
    tracing::{error, info},
};

pub async fn run_admin(addr: SocketAddr, shutdown: Arc<Notify>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "Start Admin RPC server");

    let http = ServerBuilder::new(TokioExecutor::new());
    let graceful = GracefulShutdown::new();
    loop {
        let (stream, _addr) = tokio::select! {
            () = shutdown.notified() => break,
            maybe_incoming = listener.accept() => maybe_incoming?,
        };

        let connection = http.serve_connection(
            TokioIo::new(Box::pin(stream)),
            service_fn(move |req: Request<BodyIncoming>| async move {
                let (status, body) = match req.uri().path() {
                    "/health" => (StatusCode::OK, BodyFull::new(Bytes::from("ok")).boxed()),
                    "/metrics" => (StatusCode::OK, metrics::collect_to_body()),
                    _ => (StatusCode::NOT_FOUND, BodyEmpty::new().boxed()),
                };
                Response::builder().status(status).body(body)
            }),
        );
        let fut = graceful.watch(connection.into_owned());

        tokio::spawn(async move {
            if let Err(error) = fut.await {
                error!(%error, "failed to handle connection");
            }
        });
    }

    drop(listener);
    graceful.shutdown().await;

    Ok::<(), anyhow::Error>(())
}

pub async fn run_solfees(
    addr: SocketAddr,
    body_limit: usize,
    solana_rpc: SolanaRpc,
    shutdown: Arc<Notify>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "Start Solfees RPC server");

    let http = ServerBuilder::new(TokioExecutor::new());
    let graceful = GracefulShutdown::new();
    loop {
        let (stream, _addr) = tokio::select! {
            () = shutdown.notified() => break,
            maybe_incoming = listener.accept() => maybe_incoming?,
        };

        let solana_rpc = solana_rpc.clone();
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let connection = http.serve_connection_with_upgrades(
            TokioIo::new(Box::pin(stream)),
            service_fn(move |mut req: Request<BodyIncoming>| {
                let solana_rpc = solana_rpc.clone();
                let shutdown_rx = shutdown_rx.resubscribe();
                async move {
                    let solana_rpc_mode = match req.uri().path() {
                        "/api/solana" => SolanaRpcMode::Solana,
                        "/api/solana/triton" => SolanaRpcMode::Triton,
                        "/api/solfees/ws" if is_upgrade_request(&req) => {
                            return match hyper_tungstenite::upgrade(
                                &mut req,
                                Some(WebSocketConfig {
                                    max_message_size: Some(64 * 1024), // max incoming message size: 64KiB
                                    ..Default::default()
                                }),
                            ) {
                                Ok((response, websocket)) => {
                                    tokio::spawn(solana_rpc.on_websocket(websocket));
                                    let (parts, body) = response.into_parts();
                                    Ok(Response::from_parts(parts, body.boxed()))
                                }
                                Err(error) => Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(BodyFull::new(Bytes::from(format!("{error:?}"))).boxed()),
                            };
                        }
                        _ => {
                            return Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(BodyEmpty::new().boxed())
                        }
                    };

                    call_solana_rpc(solana_rpc, solana_rpc_mode, req, body_limit, shutdown_rx).await
                }
            }),
        );
        let fut = graceful.watch(connection.into_owned());

        tokio::spawn(async move {
            let _ = fut.await;
            drop(shutdown_tx);
        });
    }

    drop(listener);
    graceful.shutdown().await;

    Ok::<(), anyhow::Error>(())
}

async fn call_solana_rpc(
    solana_rpc: SolanaRpc,
    solana_mode: SolanaRpcMode,
    request: Request<BodyIncoming>,
    body_limit: usize,
    shutdown_tx: broadcast::Receiver<()>,
) -> http::Result<Response<BoxBody<Bytes, Infallible>>> {
    match Limited::new(request.into_body(), body_limit)
        .collect()
        .map_err(|error| anyhow::anyhow!(error))
        .and_then(|body| solana_rpc.on_request(solana_mode, body.aggregate(), shutdown_tx))
        .await
    {
        Ok(body) => Response::builder()
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(BodyFull::new(Bytes::from(body)).boxed()),
        Err(error) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(BodyFull::new(Bytes::from(format!("{error}"))).boxed()),
    }
}

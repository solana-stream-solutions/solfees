use {
    crate::{
        config::ConfigMetrics,
        metrics::{self, solfees_be as metrics_be},
        rpc_solana::{SolanaRpc, SolanaRpcMode},
    },
    futures::future::TryFutureExt,
    http_body_util::{BodyExt, Empty as BodyEmpty, Full as BodyFull, Limited},
    hyper::{
        body::{Bytes, Incoming as BodyIncoming},
        header::{ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE},
        service::service_fn,
        Request, Response, StatusCode,
    },
    hyper_tungstenite::{is_upgrade_request, tungstenite::protocol::WebSocketConfig},
    hyper_util::{
        rt::tokio::{TokioExecutor, TokioIo},
        server::{conn::auto::Builder as ServerBuilder, graceful::GracefulShutdown},
    },
    std::{net::SocketAddr, sync::Arc, time::Instant},
    tokio::{
        net::TcpListener,
        sync::{broadcast, Notify},
    },
    tracing::{debug, error, info},
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
    config_metrics: Arc<ConfigMetrics>,
    shutdown: Arc<Notify>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "Start Solfees RPC server");

    let (ws_tx, _ws_rx) = broadcast::channel(1);
    let ws_tx = Arc::new(ws_tx);

    let http = ServerBuilder::new(TokioExecutor::new());
    let graceful = GracefulShutdown::new();
    loop {
        let (stream, _addr) = tokio::select! {
            () = shutdown.notified() => break,
            maybe_incoming = listener.accept() => maybe_incoming?,
        };

        let solana_rpc = solana_rpc.clone();
        let config_metrics = Arc::clone(&config_metrics);
        let ws_tx = Arc::clone(&ws_tx);
        let connection = http.serve_connection_with_upgrades(
            TokioIo::new(Box::pin(stream)),
            service_fn(move |mut req: Request<BodyIncoming>| {
                let solana_rpc = solana_rpc.clone();
                let config_metrics = Arc::clone(&config_metrics);
                let ws_tx = Arc::clone(&ws_tx);
                async move {
                    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                    enum ReqType {
                        Rpc,
                        WebSocket,
                    }

                    let (req_type, solana_rpc_mode) = match req.uri().path() {
                        "/api/solana" => (ReqType::Rpc, SolanaRpcMode::Solana),
                        "/api/solana/triton" => (ReqType::Rpc, SolanaRpcMode::Triton),
                        "/api/solana/solfees" => (ReqType::Rpc, SolanaRpcMode::Solfees),
                        "/api/solana/solfees/ws" if is_upgrade_request(&req) => {
                            (ReqType::WebSocket, SolanaRpcMode::Solfees)
                        }

                        "/api/solfees" => (ReqType::Rpc, SolanaRpcMode::SolfeesFrontend),
                        "/api/solfees/ws" if is_upgrade_request(&req) => {
                            (ReqType::WebSocket, SolanaRpcMode::SolfeesFrontend)
                        }

                        _ => {
                            return Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(BodyEmpty::new().boxed())
                        }
                    };

                    let client_id = metrics_be::ClientId::new(req.headers(), &config_metrics);
                    match req_type {
                        ReqType::Rpc => {
                            let ts = Instant::now();
                            let response = match Limited::new(req.into_body(), body_limit)
                                .collect()
                                .map_err(|error| anyhow::anyhow!(error))
                                .and_then(|body| {
                                    solana_rpc.on_request(
                                        client_id,
                                        solana_rpc_mode,
                                        body.aggregate(),
                                    )
                                })
                                .await
                            {
                                Ok((stats, body)) => {
                                    metrics_be::requests_call_inc(solana_rpc_mode, stats);
                                    Response::builder()
                                        .header(CONTENT_TYPE, "application/json; charset=utf-8")
                                        .header(ACCESS_CONTROL_ALLOW_METHODS, "OPTIONS, POST")
                                        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                                        .body(BodyFull::new(Bytes::from(body)).boxed())
                                }
                                Err(error) => Response::builder()
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(BodyFull::new(Bytes::from(format!("{error}"))).boxed()),
                            };
                            metrics_be::requests_observe(
                                solana_rpc_mode,
                                response.as_ref().map(|resp| resp.status()).ok(),
                                ts.elapsed(),
                            );
                            response
                        }
                        ReqType::WebSocket => {
                            match hyper_tungstenite::upgrade(
                                &mut req,
                                Some(WebSocketConfig {
                                    max_message_size: Some(body_limit), // max incoming message size
                                    ..Default::default()
                                }),
                            ) {
                                Ok((response, websocket)) => {
                                    tokio::spawn(solana_rpc.on_websocket(
                                        client_id,
                                        solana_rpc_mode,
                                        websocket,
                                        ws_tx.subscribe(),
                                    ));
                                    let (parts, body) = response.into_parts();
                                    Ok(Response::from_parts(parts, body.boxed()))
                                }
                                Err(error) => Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(BodyFull::new(Bytes::from(format!("{error:?}"))).boxed()),
                            }
                        }
                    }
                }
            }),
        );
        let connection = graceful.watch(connection.into_owned());
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                debug!(error, "connection error");
            }
        });
    }

    drop(listener);
    drop(ws_tx);
    graceful.shutdown().await;

    Ok::<(), anyhow::Error>(())
}

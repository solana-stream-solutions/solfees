use {
    crate::metrics::create_response as metrics_create_response,
    http_body_util::{BodyExt, Empty as BodyEmpty, Full as FullBody},
    hyper::{
        body::{Bytes, Incoming as BodyIncoming},
        service::service_fn,
        Request, Response, StatusCode,
    },
    hyper_util::{
        rt::tokio::{TokioExecutor, TokioIo},
        server::{conn::auto::Builder as ServerBuilder, graceful::GracefulShutdown},
    },
    std::{net::SocketAddr, sync::Arc},
    tokio::{net::TcpListener, sync::Notify},
    tracing::{error, info},
};

pub async fn run_admin_server(addr: SocketAddr, shutdown: Arc<Notify>) -> anyhow::Result<()> {
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
                    "/health" => (StatusCode::OK, FullBody::new(Bytes::from("ok")).boxed()),
                    "/metrics" => (StatusCode::OK, metrics_create_response()),
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

use {
    crate::{config::WithConfigTracing, tracing::init as tracing_init},
    clap::Parser,
    serde::de::DeserializeOwned,
    std::{
        future::Future,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    },
    tokio::{
        fs,
        signal::unix::{signal, SignalKind},
        sync::mpsc,
    },
};

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
pub struct Args {
    /// Path to config
    #[clap(long)]
    pub config: PathBuf,

    /// Only check config and exit
    #[clap(long, default_value_t = false)]
    pub check: bool,
}

impl Args {
    pub async fn load_config<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let contents = fs::read(&self.config).await?;
        Ok(serde_yaml::from_slice(&contents)?)
    }
}

pub fn shutdown_signal() -> mpsc::UnboundedReceiver<SignalKind> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to subscribe on SIGINT");
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to subscribe on SIGTERM");

        loop {
            let signal = tokio::select! {
                value = sigint.recv() => {
                    value.expect("failed to receive SIGINT");
                    SignalKind::interrupt()
                }
                value = sigterm.recv() => {
                    value.expect("failed to receive SIGTERM");
                    SignalKind::terminate()
                }
            };
            if tx.send(signal).is_err() {
                break;
            }
        }
    });

    rx
}

pub fn run_main<C, R>(
    metrics_init: impl Fn(),
    get_thread_name: impl Fn(u64) -> String + Send + Sync + 'static,
    main: impl Fn(C) -> R,
) -> anyhow::Result<()>
where
    C: DeserializeOwned + WithConfigTracing,
    R: Future<Output = anyhow::Result<()>>,
{
    metrics_init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name_fn(move || {
            static THREAD_ID: AtomicU64 = AtomicU64::new(0);
            let id = THREAD_ID.fetch_add(1, Ordering::Relaxed);
            get_thread_name(id)
        })
        .build()?
        .block_on(async {
            let args = Args::parse();
            let config = args.load_config::<C>().await?;
            if args.check {
                return Ok(());
            }

            tracing_init(config.get_tracing().json)?;

            main(config).await
        })
}

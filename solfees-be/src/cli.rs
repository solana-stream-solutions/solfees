use {
    clap::Parser,
    serde::de::DeserializeOwned,
    std::path::PathBuf,
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

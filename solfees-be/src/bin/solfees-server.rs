use {
    clap::Parser,
    solfees_be::{config::Config, tracing_init},
    std::path::PathBuf,
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
    pub async fn load_config(&self) -> anyhow::Result<Config> {
        Config::load(&self.config).await.map_err(Into::into)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = args.load_config().await?;
    if args.check {
        return Ok(());
    }

    tracing_init(config.tracing.json)?;

    Ok(())
}

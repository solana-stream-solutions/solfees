use {clap::Parser, solfees_be::config::ConfigServer as Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = solfees_be::cli::Args::parse();
    let config = args.load_config::<Config>().await?;
    if args.check {
        return Ok(());
    }

    solfees_be::tracing::init(config.tracing.json)?;

    Ok(())
}

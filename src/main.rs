use anyhow::Result;
use clap::Parser;
use std::time::Duration;
use tracing::debug;
use tracing_subscriber::{fmt, EnvFilter};

mod cli;
mod config;
mod mqtt_config;
mod unraid_stats;
use crate::cli::Args;
use crate::mqtt_config::MqttConfig;
use crate::unraid_stats::UnraidStats;

#[tokio::main]
async fn main() -> Result<()> {
    //LogTracer::init()?;
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(fmt::format::FmtSpan::CLOSE)
        .with_level(true)
        .with_target(true)
        .init();
    tracing::trace!("Testing trace output");
    tracing::info!("Testing info output");

    let args = Args::parse();
    let stats = UnraidStats::new(&args).await?;
    if let Some(dump_path) = &args.sensor_dump {
        debug!("Dumping sensor data to file: {}", dump_path.display());
        stats.dump_sensors_toml(dump_path).await?;
    } else if args.json_output {
        stats.publish_discovery(None).await?;
        stats.publish_stats(None).await?;
    } else {
        let config = MqttConfig::from_args_and_file(&args)?;
        let (client, mut eventloop) = config.create_mqtt_client()?;

        tokio::spawn(async move { while let Ok(_) = eventloop.poll().await {} });

        if !args.skip_discovery {
            debug!("Publishing Home Assistant discovery messages...");
            stats.publish_discovery(Some(&client)).await?;
        }

        debug!("Publishing stats...");
        stats.publish_stats(Some(&client)).await?;

        tokio::time::sleep(Duration::from_secs(2)).await;

        debug!("Stats published successfully!");
    }

    Ok(())
}

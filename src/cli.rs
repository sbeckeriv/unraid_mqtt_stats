use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// MQTT broker host
    #[arg(short = 'H', long, env = "MQTT_HOST")]
    pub host: Option<String>,

    /// MQTT broker port
    #[arg(short, long, default_value = "1883", env = "MQTT_PORT")]
    pub port: u16,

    /// MQTT client ID
    #[arg(short = 'i', long, env = "MQTT_CLIENT_ID")]
    pub client_id: Option<String>,

    /// MQTT username
    #[arg(short, long, env = "MQTT_USER")]
    pub username: Option<String>,

    /// MQTT password
    #[arg(short = 'P', long, env = "MQTT_PASSWORD")]
    pub password: Option<String>,

    /// Toml configuration file for sensors
    #[arg(short = 'c', long)]
    pub config_file: Option<PathBuf>,

    /// Dump overwriteable sensor settings to file. You cant change how the default sensors work.
    #[arg(long)]
    pub sensor_dump: Option<PathBuf>,

    /// JSON output mode (outputs stats to stdout instead of MQTT)
    #[arg(long)]
    pub json_output: bool,

    /// Home Assistant discovery prefix
    #[arg(long, default_value = "homeassistant")]
    pub discovery_prefix: String,

    /// Device name for Home Assistant
    #[arg(long, default_value = "unraid")]
    pub device_name: String,

    /// Skip Home Assistant discovery messages
    #[arg(long)]
    pub skip_discovery: bool,
}

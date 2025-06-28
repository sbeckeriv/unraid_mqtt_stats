use crate::cli::Args;
use anyhow::Result;
use rumqttc::{AsyncClient, EventLoop, MqttOptions};
use std::time::Duration;

#[derive(Debug)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub username: String,
    pub password: String,
}

impl MqttConfig {
    pub fn from_args_and_file(args: &Args) -> anyhow::Result<Self> {
        let mut config = MqttConfig {
            host: String::new(),
            port: args.port,
            client_id: String::new(),
            username: String::new(),
            password: String::new(),
        };

        if let Some(host) = &args.host {
            config.host = host.clone();
        }
        if let Some(client_id) = &args.client_id {
            config.client_id = client_id.clone();
        }
        if let Some(username) = &args.username {
            config.username = username.clone();
        }
        if let Some(password) = &args.password {
            config.password = password.clone();
        }

        if config.host.is_empty() {
            anyhow::bail!(
                "MQTT host is required. Set via --host, MQTT_HOST env var, or config file"
            );
        }
        if config.client_id.is_empty() {
            config.client_id = format!("unraid-mqtt-stats-{}", std::process::id());
        }

        Ok(config)
    }

    pub fn create_mqtt_client(&self) -> Result<(AsyncClient, EventLoop)> {
        let mut mqtt_options = MqttOptions::new(&self.client_id, &self.host, self.port);

        if !self.username.is_empty() && !self.password.is_empty() {
            mqtt_options.set_credentials(&self.username, &self.password);
        }

        mqtt_options.set_keep_alive(Duration::from_secs(5));

        let (client, eventloop) = AsyncClient::new(mqtt_options, 10);
        Ok((client, eventloop))
    }
}

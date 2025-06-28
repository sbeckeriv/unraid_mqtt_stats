use crate::cli::Args;
use crate::config::{
    self, CommandSensorReporter, Config, DeviceClass, DockerContainerSensorReporter,
    DockerContainerSensorReporterStat, DockerSensorReporter, DockerSensorReporterStat, Sensor,
    SensorReporterType, Sensors, SensorsDump, SystemSensorReporter, SystemSensorReporterStat,
};
use anyhow::Result;
use bollard::query_parameters::ListContainersOptions;
use bollard::secret::ContainerSummary;
use bollard::Docker;
use rumqttc::{AsyncClient, QoS};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use sysinfo::System;
use tokio::sync::Mutex;
use tracing::{debug, instrument};

#[derive(Debug)]
pub struct UnraidStats {
    sensor_config: Option<Config>,
    docker: Docker,
    json_output: bool,
    discovery_prefix: String,
    device_name: String,
    skip_discovery: bool,
}

impl UnraidStats {
    pub async fn new(args: &Args) -> Result<Self> {
        let docker = Docker::connect_with_socket_defaults()?;
        let sensor_config = args
            .config_file
            .as_ref()
            .map(|file| config::load_config(file));

        Ok(UnraidStats {
            sensor_config,
            docker,
            json_output: args.json_output,
            discovery_prefix: args.discovery_prefix.clone(),
            device_name: args.device_name.clone(),
            skip_discovery: args.skip_discovery,
        })
    }

    pub async fn dump_sensors_toml(&self, filename: &PathBuf) -> Result<()> {
        let dump_sensors = self
            .sensors()
            .await
            .into_iter()
            .map(|s| (s.id.clone(), SensorsDump::SensorOverride(s)))
            .collect::<HashMap<String, SensorsDump>>();
        let dump = config::ConfigDump {
            sensors: dump_sensors,
        };
        let toml_string = toml::to_string(&dump)?;
        fs::write(filename, toml_string)?;
        Ok(())
    }

    fn get_device_info(&self) -> serde_json::Value {
        json!({
            "identifiers": [format!("unraid_{}", self.device_name)],
            "name": format!("Unraid {}", self.device_name),
            "model": "Unraid Server",
            "manufacturer": "Lime Technology",
            "sw_version": self.get_unraid_version().unwrap_or_else(|_| "Unknown".to_string())
        })
    }

    fn get_unraid_version(&self) -> Result<String> {
        let content = fs::read_to_string("/etc/unraid-version")?;
        for line in content.lines() {
            if line.starts_with("version=") {
                return Ok(line
                    .trim_start_matches("version=")
                    .trim_matches('"')
                    .to_string());
            }
        }
        Ok("Unknown".to_string())
    }

    pub async fn sensors(&self) -> Vec<Sensor> {
        let mut containters = self
            .containers()
            .await
            .unwrap_or_default()
            .into_iter()
            .flat_map(|container| self.container_sensors(container))
            .collect::<Vec<Sensor>>();

        let mut sys = System::new_all();
        sys.refresh_all();

        let mut sensors = vec![
            Sensor {
                id: "cpu_usage".to_string(),
                name: "CPU Usage".to_string(),
                unit: Some("%".to_string()),
                reporter: Some(SensorReporterType::System(SystemSensorReporter {
                    system: Arc::new(System::new_all()),
                    name: SystemSensorReporterStat::CpuUsage,
                })),
                ..Default::default()
            },
            Sensor {
                id: "memory_usage".to_string(),
                name: "Memory Usage".to_string(),
                unit: Some("%".to_string()),
                reporter: Some(SensorReporterType::System(SystemSensorReporter {
                    system: Arc::new(System::new_all()),
                    name: SystemSensorReporterStat::MemoryUsage,
                })),
                ..Default::default()
            },
            Sensor {
                id: "memory_total".to_string(),
                name: "Memory Total".to_string(),
                unit: Some("B".to_string()),
                device_class: Some(DeviceClass::DataSize),
                icon: Some("memory".to_string()),
                reporter: Some(SensorReporterType::System(SystemSensorReporter {
                    system: Arc::new(System::new_all()),
                    name: SystemSensorReporterStat::MemoryTotal,
                })),
                ..Default::default()
            },
            Sensor {
                id: "memory_used".to_string(),
                name: "Memory Used".to_string(),
                unit: Some("B".to_string()),
                device_class: Some(DeviceClass::DataSize),
                icon: Some("memory".to_string()),
                reporter: Some(SensorReporterType::System(SystemSensorReporter {
                    system: Arc::new(System::new_all()),
                    name: SystemSensorReporterStat::MemoryUsed,
                })),
                ..Default::default()
            },
            Sensor {
                id: "disk_usage".to_string(),
                name: "Disk Usage".to_string(),
                unit: Some("%".to_string()),
                reporter: Some(SensorReporterType::Command(CommandSensorReporter {
                    command: "df".to_string(),
                    args: Some(vec!["-BM".to_string(), "/mnt/user".to_string()]),
                    transform: Some(Arc::new(|s: &str| {
                        if let Some(disk_info) = parse_disk_usage(&s) {
                            Some(format!("{}", disk_info.usage_percent))
                        } else {
                            None
                        }
                    })),
                })),
                ..Default::default()
            },
            Sensor {
                id: "disk_total".to_string(),
                name: "Disk Total".to_string(),
                unit: Some("B".to_string()),
                device_class: Some(DeviceClass::DataSize),
                icon: Some("data_size".to_string()),
                reporter: Some(SensorReporterType::Command(CommandSensorReporter {
                    command: "df".to_string(),
                    args: Some(vec!["/mnt/user".to_string()]),
                    transform: Some(Arc::new(|s: &str| {
                        if let Some(disk_info) = parse_disk_usage(&s) {
                            debug!("Disk info: {:?}", disk_info);
                            Some(disk_info.total.to_string())
                        } else {
                            None
                        }
                    })),
                })),
                ..Default::default()
            },
            Sensor {
                id: "disk_available".to_string(),
                name: "Disk Available".to_string(),
                unit: Some("B".to_string()),
                device_class: Some(DeviceClass::DataSize),
                icon: Some("data_size".to_string()),
                reporter: Some(SensorReporterType::Command(CommandSensorReporter {
                    command: "df".to_string(),
                    args: Some(vec!["/mnt/user".to_string()]),
                    transform: Some(Arc::new(|s: &str| {
                        if let Some(disk_info) = parse_disk_usage(&s) {
                            Some(format!("{}", disk_info.available))
                        } else {
                            None
                        }
                    })),
                })),
                ..Default::default()
            },
            Sensor {
                id: "cpu_temp".to_string(),
                name: "CPU Temperature".to_string(),
                unit: Some("°C".to_string()),
                device_class: Some(DeviceClass::Temperature),
                reporter: Some(SensorReporterType::Command(CommandSensorReporter {
                    command: "sensor".to_string(),
                    args: None,
                    transform: Some(Arc::new(|s: &str| {
                        if let Some(temp) = parse_cpu_temp(&s) {
                            Some(format!("{:.1}", temp))
                        } else {
                            None
                        }
                    })),
                })),
                ..Default::default()
            },
            Sensor {
                id: "uptime".to_string(),
                name: "Uptime".to_string(),
                icon: Some("duration".to_string()),
                reporter: Some(SensorReporterType::System(SystemSensorReporter {
                    system: Arc::new(System::new_all()),
                    name: SystemSensorReporterStat::Uptime,
                })),
                ..Default::default()
            },
            Sensor {
                id: "array_status".to_string(),
                name: "Array Status".to_string(),
                //Command::new("mdcmd").arg("status")
                reporter: Some(SensorReporterType::Command(CommandSensorReporter {
                    command: "mdcmd".to_string(),
                    args: Some(vec!["status".to_string()]),
                    transform: Some(Arc::new(|s: &str| {
                        if let Some(status) = parse_array_status(&s) {
                            Some(status)
                        } else {
                            None
                        }
                    })),
                })),
                ..Default::default()
            },
            Sensor {
                id: "docker_containers_running".to_string(),
                name: "Docker Containers Running".to_string(),
                icon: Some("docker".to_string()),
                reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                    stat: DockerSensorReporterStat::RunningCount,
                    docker: Arc::new(self.docker.clone()),
                })),
                ..Default::default()
            },
            Sensor {
                id: "docker_containers_unhealthy".to_string(),
                name: "Docker Containers Unhealthy".to_string(),
                icon: Some("docker".to_string()),
                reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                    stat: DockerSensorReporterStat::UnhealthyCount,
                    docker: Arc::new(self.docker.clone()),
                })),
                ..Default::default()
            },
            Sensor {
                id: "docker_images_count".to_string(),
                name: "Docker Images".to_string(),
                icon: Some("docker".to_string()),
                reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                    stat: DockerSensorReporterStat::ImagesCount,
                    docker: Arc::new(self.docker.clone()),
                })),
                ..Default::default()
            },
            Sensor {
                id: "docker_images_size".to_string(),
                name: "Docker Images Size".to_string(),
                icon: Some("data_size".to_string()),
                device_class: Some(DeviceClass::DataSize),
                unit: Some("B".to_string()),
                reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                    stat: DockerSensorReporterStat::ImagesSize,
                    docker: Arc::new(self.docker.clone()),
                })),
                ..Default::default()
            },
            Sensor {
                id: "docker_volumes_count".to_string(),
                name: "Docker Volumes".to_string(),
                icon: Some("docker".to_string()),
                reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                    stat: DockerSensorReporterStat::VolumesCount,
                    docker: Arc::new(self.docker.clone()),
                })),
                ..Default::default()
            },
        ];
        sensors.append(&mut containters);
        if let Some(sensor_config) = self.sensor_config.as_ref() {
            for sensor in sensors.iter_mut() {
                // apply star overrides then named overrides
                let mut star_name = sensor.id.split('_');
                let star_id = format!(
                    "{}_*_{}",
                    star_name.nth(0).unwrap_or(""),
                    star_name.last().unwrap_or("")
                );
                if let Some(Sensors::SensorOverride(update)) =
                    sensor_config.sensors.get(star_id.as_str())
                {
                    sensor.merge(update);
                }

                if let Some(Sensors::SensorOverride(update)) =
                    sensor_config.sensors.get(sensor.id.as_str())
                {
                    sensor.merge(update);
                }
            }
            for sensor in sensor_config.sensors.values() {
                if let Sensors::Command(command) = sensor {
                    sensors.push(command.into());
                }
            }
        }
        sensors
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn publish_discovery(&self, client: Option<&AsyncClient>) -> Result<()> {
        if self.skip_discovery {
            return Ok(());
        }

        let device_info = self.get_device_info();
        let node_id = format!("unraid_{}", self.device_name);

        for sensor in self.sensors().await {
            if sensor.disabled {
                continue;
            }
            let discovery_topic = sensor.discovery_topic(&self.discovery_prefix, &node_id);
            let config = sensor.disovery_config(&self.device_name, &node_id, &device_info);
            self.publish_raw(client, &discovery_topic, config.to_string(), true)
                .await?;
        }

        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn publish_stats(&self, client: Option<&AsyncClient>) -> Result<()> {
        let node_id = format!("unraid_{}", self.device_name);
        for sensor in self.sensors().await {
            if sensor.disabled {
                continue;
            }
            let sensor_topic = sensor.sensor_topic(&node_id);
            if let Some(mut source) = sensor.reporter {
                if let Some(value) = source.get_value().await {
                    debug!("Sensor ID: {}, Value: {}", sensor.id, value);
                    self.publish_ha_state(client, &sensor_topic, value).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn containers(&self) -> Result<Vec<ContainerSummary>> {
        let mut filters = HashMap::new();
        filters.insert("status".into(), vec!["running".into()]);
        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: Some(filters),
                ..Default::default()
            }))
            .await?;
        Ok(containers)
    }

    fn container_sensors(&self, container: ContainerSummary) -> Vec<Sensor> {
        let container = Arc::new(container);
        let container_name = container
            .names
            .as_ref()
            .and_then(|names| names.first())
            .map(|n| n.trim_start_matches('/'))
            .unwrap_or("unknown");
        let stats_stash = Arc::new(Mutex::new(None));
        vec![
            Sensor {
                id: format!("dockercontainer_{}_cpu", container_name),
                name: format!("{} Docker {} CPU", self.device_name, container_name),
                icon: Some("mdi:cpu-64-bit".to_string()),
                unit: Some("%".to_string()),
                reporter: Some(SensorReporterType::DockerContainer(
                    DockerContainerSensorReporter {
                        container: container.clone(),
                        stats_stash: stats_stash.clone(),
                        stat: DockerContainerSensorReporterStat::CpuUsage,
                        docker: Arc::new(self.docker.clone()),
                    },
                )),
                ..Default::default()
            },
            Sensor {
                id: format!("dockercontainer_{}_memory", container_name),
                name: format!("{} Docker {} Memory", self.device_name, container_name),
                icon: Some("mdi:memory".to_string()),
                unit: Some("B".to_string()),
                device_class: Some(DeviceClass::DataSize),
                reporter: Some(SensorReporterType::DockerContainer(
                    DockerContainerSensorReporter {
                        container: container.clone(),
                        stats_stash: stats_stash.clone(),
                        stat: DockerContainerSensorReporterStat::MemoryUsage,
                        docker: Arc::new(self.docker.clone()),
                    },
                )),
                ..Default::default()
            },
            Sensor {
                id: format!("dockercontainer_{}_uptime", container_name),
                name: format!("{} Docker {} Uptime", self.device_name, container_name),
                icon: Some("mdi:docker".to_string()),
                reporter: Some(SensorReporterType::DockerContainer(
                    DockerContainerSensorReporter {
                        container: container.clone(),
                        stats_stash: stats_stash.clone(),
                        stat: DockerContainerSensorReporterStat::Status,
                        docker: Arc::new(self.docker.clone()),
                    },
                )),
                ..Default::default()
            },
        ]
    }

    #[instrument(level = "trace", skip(self, client))]
    async fn publish_ha_state(
        &self,
        client: Option<&AsyncClient>,
        topic_suffix: &str,
        value: String,
    ) -> Result<()> {
        if self.json_output {
            println!(
                "{}",
                json!({
                    "topic": topic_suffix,
                    "payload": value
                })
            );
        } else if let Some(client) = client {
            self.publish_raw(Some(client), &topic_suffix, value, false)
                .await?;
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self, client))]
    async fn publish_raw(
        &self,
        client: Option<&AsyncClient>,
        topic: &str,
        payload: String,
        retain: bool,
    ) -> Result<()> {
        if self.json_output {
            println!(
                "{}",
                json!({
                    "topic": topic,
                    "payload": payload,
                })
            );
        } else if let Some(client) = client {
            client
                .publish(topic, QoS::AtLeastOnce, retain, payload)
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct DiskInfo {
    total: String,
    available: String,
    usage_percent: f64,
}

fn parse_disk_usage(df_output: &str) -> Option<DiskInfo> {
    df_output.lines().skip(1).next().and_then(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let usage_str = parts[4].trim_end_matches('%');

            let usage_percent = usage_str.parse::<f64>().ok()?;
            Some(DiskInfo {
                total: parts[1].to_string(),
                available: parts[3].to_string(),
                usage_percent,
            })
        } else {
            None
        }
    })
}

fn parse_cpu_temp(sensors_output: &str) -> Option<f64> {
    sensors_output
        .lines()
        .find(|line| line.contains("Package id 0"))
        .and_then(|line| {
            line.split_whitespace()
                .find(|word| word.contains("°C"))
                .and_then(|temp| {
                    temp.trim_start_matches('+')
                        .trim_end_matches("°C")
                        .parse::<f64>()
                        .ok()
                })
        })
}

fn parse_array_status(status_output: &str) -> Option<String> {
    status_output
        .lines()
        .find(|line| line.starts_with("mdState="))
        .map(|line| line.trim_start_matches("mdState=").to_string())
}

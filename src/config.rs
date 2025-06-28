use bollard::{
    query_parameters::{
        ListContainersOptions, ListImagesOptions, ListVolumesOptions, StatsOptions,
    },
    secret::{ContainerStatsResponse, ContainerSummary},
    Docker,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, process::Command, sync::Arc};
use sysinfo::System;
use tracing::instrument;

pub fn load_config(file: &PathBuf) -> Config {
    let content = std::fs::read_to_string(file).expect("Failed to read config file");
    toml::from_str(&content).expect("Failed to parse config file")
}

#[derive(Serialize, Default, Deserialize, Debug)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_sensors")]
    pub sensors: HashMap<String, Sensors>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Sensors {
    #[serde(rename = "override")]
    SensorOverride(SensorConfig),
    #[serde(rename = "command")]
    Command(CommandSensor),
}

#[derive(Serialize, Default, Deserialize, Debug)]
pub struct SensorConfig {
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: Option<String>,
    pub unit: Option<String>,
    pub device_class: Option<DeviceClass>,
    pub icon: Option<String>,
    pub disabled: bool,
}

#[derive(Serialize, Default, Deserialize)]
pub struct ConfigDump {
    #[serde(
        serialize_with = "dump_serialize_sensors",
        deserialize_with = "dump_deserialize_sensors"
    )]
    pub sensors: HashMap<String, SensorsDump>,
}

fn dump_serialize_sensors<S>(
    sensors: &HashMap<String, SensorsDump>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // The sensors already have their id in the HashMap key, so just serialize normally
    sensors.serialize(serializer)
}

fn dump_deserialize_sensors<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, SensorsDump>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut sensors: HashMap<String, SensorsDump> = HashMap::deserialize(deserializer)?;

    for (id, sensor) in sensors.iter_mut() {
        match sensor {
            SensorsDump::SensorOverride(s) => s.id = id.clone(),
        }
    }

    Ok(sensors)
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SensorsDump {
    #[serde(rename = "override")]
    SensorOverride(Sensor),
}
#[derive(Serialize, Default, Deserialize)]
pub struct Sensor {
    #[serde(skip_deserializing, skip_serializing)]
    pub id: String,
    pub name: String,
    pub unit: Option<String>,
    pub device_class: Option<DeviceClass>,
    pub icon: Option<String>,
    pub disabled: bool,
    #[serde(skip, default)]
    pub reporter: Option<SensorReporterType>,
}

#[derive(Serialize, Default, Deserialize, Debug)]
pub struct CommandSensor {
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    pub unit: Option<String>,
    pub device_class: Option<DeviceClass>,
    pub icon: Option<String>,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub post_process: Option<PostProcess>,
    pub disabled: bool,
}
#[derive(Serialize, Deserialize, Debug)]
pub enum PostProcess {
    TrimWhitespace,
    ParseFloat,
    ParseInteger,
    ExtractNumber,
    ToUpperCase,
    ToLowerCase,
}
impl From<&CommandSensor> for Sensor {
    fn from(command_sensor: &CommandSensor) -> Self {
        Sensor {
            id: command_sensor.id.clone(),
            name: command_sensor.name.clone(),
            unit: command_sensor.unit.clone(),
            device_class: command_sensor.device_class.clone(),
            icon: command_sensor.icon.clone(),
            disabled: command_sensor.disabled,
            reporter: Some(SensorReporterType::Command(CommandSensorReporter {
                command: command_sensor.command.clone(),
                args: command_sensor.args.clone(),
                transform: match command_sensor.post_process {
                    Some(PostProcess::TrimWhitespace) => {
                        Some(Arc::new(|s| Some(s.trim().to_string())))
                    }
                    Some(PostProcess::ParseFloat) => {
                        Some(Arc::new(|s| s.parse::<f64>().ok().map(|v| v.to_string())))
                    }
                    Some(PostProcess::ParseInteger) => {
                        Some(Arc::new(|s| s.parse::<i64>().ok().map(|v| v.to_string())))
                    }
                    Some(PostProcess::ExtractNumber) => Some(Arc::new(|s| {
                        s.chars()
                            .filter(|c| c.is_numeric())
                            .collect::<String>()
                            .parse::<f64>()
                            .ok()
                            .map(|v| v.to_string())
                    })),
                    Some(PostProcess::ToUpperCase) => Some(Arc::new(|s| Some(s.to_uppercase()))),
                    Some(PostProcess::ToLowerCase) => Some(Arc::new(|s| Some(s.to_lowercase()))),
                    None => Some(Arc::new(|s| Some(s.to_string()))),
                },
            })),
        }
    }
}

fn deserialize_sensors<'de, D>(deserializer: D) -> Result<HashMap<String, Sensors>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut sensors: HashMap<String, Sensors> = HashMap::deserialize(deserializer)?;

    for (id, sensor) in sensors.iter_mut() {
        match sensor {
            Sensors::SensorOverride(s) => s.id = id.clone(),
            Sensors::Command(s) => s.id = id.clone(),
        }
    }

    Ok(sensors)
}
/*
#[derive(Serialize, Default, Deserialize)]
pub struct Config {
    pub sensors: Vec<Sensor>,
}
#[derive(Serialize, Default, Deserialize)]
pub struct Sensor {
    pub id: String,
    pub name: String,
    pub unit: Option<String>,
    pub device_class: Option<DeviceClass>,
    pub icon: Option<String>,
    pub disabled: bool,
    #[serde(skip, default)]
    pub reporter: Option<SensorReporterType>,
}
 */
impl Sensor {
    // merge from config file
    pub fn merge(&mut self, other: &SensorConfig) {
        if self.id != other.id && !other.id.contains("_*_") {
            return;
        }
        if let Some(name) = &other.name {
            self.name = name.clone();
        }
        if other.unit.is_some() {
            self.unit = other.unit.clone();
        }
        if other.device_class.is_some() {
            self.device_class = other.device_class.clone();
        }
        if other.icon.is_some() {
            self.icon = other.icon.clone();
        }
        if other.disabled {
            self.disabled = other.disabled;
        }
    }

    pub fn sensor_topic(&self, node_id: &str) -> String {
        format!("{}/sensor/{}/state", node_id, self.id)
    }
    pub fn discovery_topic(&self, discovery_prefix: &str, node_id: &str) -> String {
        format!("{}/sensor/{}/{}/config", discovery_prefix, node_id, self.id)
    }
    pub fn disovery_config(&self, device_name: &str, node_id: &str, device_info: &Value) -> Value {
        let mut config = json!({
            "name": format!("{} {}", device_name, self.name),
            "state_topic": self.sensor_topic(node_id),
            "unique_id": format!("{}_{}", node_id, self.id),
            "device": device_info,
            "unit_of_measurement": self.unit,
        });

        if let Some(device_class) = &self.device_class {
            config["device_class"] = json!(device_class);
        }
        if let Some(icon_str) = &self.icon {
            config["icon"] = json!(format!("mdi:{}", icon_str));
        }

        config
    }
}

pub enum SensorReporterType {
    System(SystemSensorReporter),
    Command(CommandSensorReporter),
    DockerContainer(DockerContainerSensorReporter),
    Docker(DockerSensorReporter),
}
impl SensorReporterType {
    pub async fn get_value(&mut self) -> Option<String> {
        match self {
            SensorReporterType::System(reporter) => reporter.get_value().await,
            SensorReporterType::Command(reporter) => reporter.get_value().await,
            SensorReporterType::DockerContainer(reporter) => reporter.get_value().await,
            SensorReporterType::Docker(reporter) => reporter.get_value().await,
        }
    }
}
pub struct CommandSensorReporter {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub transform: Option<Arc<dyn Fn(&str) -> Option<String> + Send + Sync>>,
}

impl CommandSensorReporter {
    #[instrument(level = "trace", skip(self))]
    async fn get_value(&mut self) -> Option<String> {
        let mut command = Command::new(&self.command);
        if let Some(args) = &self.args {
            command.args(args);
        }
        if let Ok(output) = command.output() {
            let sensors_output = String::from_utf8_lossy(&output.stdout);
            let result = sensors_output.trim();
            if let Some(transform_fn) = &self.transform {
                transform_fn(result)
            } else {
                Some(result.to_string())
            }
        } else {
            None
        }
    }
}
pub enum SystemSensorReporterStat {
    MemoryUsage,
    MemoryUsed,
    MemoryTotal,
    CpuUsage,
    Uptime,
}
pub struct SystemSensorReporter {
    pub system: Arc<System>,
    pub name: SystemSensorReporterStat,
}

impl SystemSensorReporter {
    #[instrument(level = "trace", skip(self), name = "SystemSesnsorReporter::get_value")]
    async fn get_value(&self) -> Option<String> {
        match self.name {
            SystemSensorReporterStat::MemoryUsage => {
                let total_memory = self.system.total_memory() as f64;
                let used_memory = self.system.used_memory() as f64;
                Some(format!("{:.1}", (used_memory / total_memory) * 100.0))
            }
            SystemSensorReporterStat::MemoryUsed => {
                Some(format!("{:.1}", self.system.used_memory()))
            }
            SystemSensorReporterStat::MemoryTotal => {
                Some(format!("{:.1}", self.system.total_memory()))
            }
            SystemSensorReporterStat::CpuUsage => {
                let cpu_usage = self.system.global_cpu_usage();
                Some(format!("{:.1}", cpu_usage))
            }
            SystemSensorReporterStat::Uptime => Some(format!("{}", System::uptime())),
        }
    }
}

pub enum DockerSensorReporterStat {
    ImagesCount,
    ImagesSize,
    VolumesCount,
    RunningCount,
    UnhealthyCount,
}
pub struct DockerSensorReporter {
    pub docker: Arc<Docker>,
    pub stat: DockerSensorReporterStat,
}

impl DockerSensorReporter {
    #[instrument(level = "trace", skip(self), name = "DockerSesnsorReporter::get_value")]
    async fn get_value(&self) -> Option<String> {
        async fn list_containers(
            docker: &Docker,
            filters: HashMap<String, Vec<String>>,
        ) -> Option<String> {
            let containers = docker
                .list_containers(Some(ListContainersOptions {
                    all: true,
                    filters: Some(filters),
                    ..Default::default()
                }))
                .await;
            if let Ok(containers) = containers {
                Some(containers.len().to_string())
            } else {
                None
            }
        }
        match self.stat {
            DockerSensorReporterStat::ImagesCount => {
                let images = self
                    .docker
                    .list_images(Some(ListImagesOptions::default()))
                    .await;
                if let Ok(images) = images {
                    Some(images.len().to_string())
                } else {
                    None
                }
            }
            DockerSensorReporterStat::ImagesSize => {
                let images = self
                    .docker
                    .list_images(Some(ListImagesOptions::default()))
                    .await;
                if let Ok(images) = images {
                    let total_size: i64 = images.iter().map(|i| i.size).sum();
                    Some((total_size).to_string())
                } else {
                    None
                }
            }
            DockerSensorReporterStat::VolumesCount => {
                let filter = ListVolumesOptions { filters: None };
                let volumes = self.docker.list_volumes(Some(filter)).await;
                if let Ok(volumes) = volumes {
                    let volume_count = volumes
                        .volumes
                        .as_ref()
                        .map(|v| v.len())
                        .unwrap_or_default();
                    Some(volume_count.to_string())
                } else {
                    None
                }
            }
            DockerSensorReporterStat::RunningCount => {
                let mut filters = HashMap::new();
                filters.insert("status".into(), vec!["running".into()]);
                list_containers(&self.docker, filters).await
            }
            DockerSensorReporterStat::UnhealthyCount => {
                let mut filters = HashMap::new();
                filters.insert("health".into(), vec!["unhealthy".into()]);
                list_containers(&self.docker, filters).await
            }
        }
    }
}
pub enum DockerContainerSensorReporterStat {
    CpuUsage,
    MemoryUsage,
    Status,
}
pub struct DockerContainerSensorReporter {
    pub container: Arc<ContainerSummary>,
    pub docker: Arc<Docker>,
    pub stats_stash: Arc<tokio::sync::Mutex<Option<ContainerStatsResponse>>>,
    pub stat: DockerContainerSensorReporterStat,
}

impl DockerContainerSensorReporter {
    #[instrument(
        level = "trace",
        skip(self),
        name = "DockerContainerSesnsorReporter::get_value"
    )]
    async fn get_value(&self) -> Option<String> {
        if self.stats_stash.lock().await.is_none() {
            let mut stats_stream = self.docker.stats(
                &self.container.id.as_ref().unwrap(),
                Some(StatsOptions {
                    stream: true,
                    one_shot: false,
                }),
            );
            if let Some(Ok(stats)) = stats_stream.next().await {
                self.stats_stash.lock().await.replace(stats.clone());
            }
        }
        if let Some(stats) = self.stats_stash.lock().await.clone() {
            match self.stat {
                DockerContainerSensorReporterStat::CpuUsage => {
                    let cpu_percent = calculate_cpu_percent(&stats);
                    Some(format!("{}", cpu_percent))
                }
                DockerContainerSensorReporterStat::MemoryUsage => {
                    let memory_usage = stats
                        .memory_stats
                        .map(|m| m.usage)
                        .flatten()
                        .unwrap_or_default();
                    Some(format!("{}", memory_usage))
                }
                DockerContainerSensorReporterStat::Status => {
                    if let Some(status) = &self.container.status {
                        Some(status.clone())
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    Date,
    Enum,
    Timestamp,
    ApparentPower,
    Aqi,
    Area,
    AtmosphericPressure,
    Battery,
    BloodGlucoseConcentration,
    CarbonMonoxide,
    CarbonDioxide,
    Conductivity,
    Current,
    DataRate,
    DataSize,
    Distance,
    Duration,
    Energy,
    EnergyDistance,
    EnergyStorage,
    Frequency,
    Gas,
    Humidity,
    Illuminance,
    Irradiance,
    Moisture,
    Monetary,
    NitrogenDioxide,
    NitrogenMonoxide,
    NitrousOxide,
    Ozone,
    Ph,
    Pm1,
    Pm10,
    Pm25,
    PowerFactor,
    Power,
    Precipitation,
    PrecipitationIntensity,
    Pressure,
    ReactiveEnergy,
    ReactivePower,
    SignalStrength,
    SoundPressure,
    Speed,
    SulphurDioxide,
    Temperature,
    VolatileOrganicCompounds,
    VolatileOrganicCompoundsParts,
    Voltage,
    Volume,
    VolumeStorage,
    VolumeFlowRate,
    Water,
    Weight,
    WindDirection,
    WindSpeed,
}

// https://github.com/home-assistant/core/blob/dev/homeassistant/const.py#L619

pub fn calculate_cpu_percent(stats: &ContainerStatsResponse) -> f64 {
    let cpu_stats = &stats.cpu_stats;
    let precpu_stats = &stats.precpu_stats;

    let cpu_delta = cpu_stats
        .as_ref()
        .and_then(|c| c.cpu_usage.as_ref().and_then(|c| c.total_usage))
        .unwrap_or_default()
        - precpu_stats
            .as_ref()
            .and_then(|c| c.cpu_usage.as_ref().and_then(|c| c.total_usage))
            .unwrap_or_default();
    let system_delta = cpu_stats
        .as_ref()
        .and_then(|c| c.system_cpu_usage)
        .unwrap_or_default()
        - precpu_stats
            .as_ref()
            .and_then(|c| c.system_cpu_usage)
            .unwrap_or_default();

    if system_delta > 0 && cpu_delta > 0 {
        let cpu_count = cpu_stats
            .as_ref()
            .and_then(|c| c.online_cpus)
            .unwrap_or_default() as f64;
        (cpu_delta as f64 / system_delta as f64) * cpu_count * 100.0
    } else {
        0.0
    }
}

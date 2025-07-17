use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use bollard::{query_parameters::ListContainersOptions, secret::ContainerSummary, Docker};
use tokio::sync::Mutex;

use crate::config::{
    DeviceClass, DockerContainerSensorReporter, DockerContainerSensorReporterStat,
    DockerSensorReporter, DockerSensorReporterStat, Sensor, SensorReporterType,
};

pub async fn sensor_list(docker: &Docker) -> Vec<Sensor> {
    vec![
        Sensor {
            id: "docker_containers_running".to_string(),
            name: "Docker Containers Running".to_string(),
            icon: Some("docker".to_string()),
            reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                stat: DockerSensorReporterStat::RunningCount,
                docker: Arc::new(docker.clone()),
            })),
            ..Default::default()
        },
        Sensor {
            id: "docker_containers_unhealthy".to_string(),
            name: "Docker Containers Unhealthy".to_string(),
            icon: Some("docker".to_string()),
            reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                stat: DockerSensorReporterStat::UnhealthyCount,
                docker: Arc::new(docker.clone()),
            })),
            ..Default::default()
        },
        Sensor {
            id: "docker_images_count".to_string(),
            name: "Docker Images".to_string(),
            icon: Some("docker".to_string()),
            reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                stat: DockerSensorReporterStat::ImagesCount,
                docker: Arc::new(docker.clone()),
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
                docker: Arc::new(docker.clone()),
            })),
            ..Default::default()
        },
        Sensor {
            id: "docker_volumes_count".to_string(),
            name: "Docker Volumes".to_string(),
            icon: Some("docker".to_string()),
            reporter: Some(SensorReporterType::Docker(DockerSensorReporter {
                stat: DockerSensorReporterStat::VolumesCount,
                docker: Arc::new(docker.clone()),
            })),
            ..Default::default()
        },
    ]
}

pub async fn container_sensor_list(docker: &Docker, device_name: &str) -> Result<Vec<Sensor>> {
    Ok(containers(docker)
        .await?
        .into_iter()
        .flat_map(|container| container_sensors(docker, device_name, container))
        .collect::<Vec<Sensor>>())
}
pub async fn containers(docker: &Docker) -> Result<Vec<ContainerSummary>> {
    let mut filters = HashMap::new();
    filters.insert("status".into(), vec!["running".into()]);
    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: Some(filters),
            ..Default::default()
        }))
        .await?;
    Ok(containers)
}

fn container_sensors(
    docker: &Docker,
    device_name: &str,
    container: ContainerSummary,
) -> Vec<Sensor> {
    let container = Arc::new(container);
    let container_name = container
        .names
        .as_ref()
        .and_then(|names| names.first())
        .map(|n| n.trim_start_matches('/'))
        .unwrap_or("unknown");
    let stats_stash = Arc::new(Mutex::new(None));
    // https://docs.rs/bollard/latest/bollard/models/struct.ContainerStatsResponse.html
    vec![
        Sensor {
            id: format!("dockercontainer_{}_cpu", container_name),
            name: format!("{} Docker {} CPU", device_name, container_name),
            icon: Some("mdi:cpu-64-bit".to_string()),
            unit: Some("%".to_string()),
            reporter: Some(SensorReporterType::DockerContainer(
                DockerContainerSensorReporter {
                    container: container.clone(),
                    stats_stash: stats_stash.clone(),
                    stat: DockerContainerSensorReporterStat::CpuUsage,
                    docker: Arc::new(docker.clone()),
                },
            )),
            ..Default::default()
        },
        Sensor {
            id: format!("dockercontainer_{}_memory", container_name),
            name: format!("{} Docker {} Memory", device_name, container_name),
            icon: Some("mdi:memory".to_string()),
            unit: Some("B".to_string()),
            device_class: Some(DeviceClass::DataSize),
            reporter: Some(SensorReporterType::DockerContainer(
                DockerContainerSensorReporter {
                    container: container.clone(),
                    stats_stash: stats_stash.clone(),
                    stat: DockerContainerSensorReporterStat::MemoryUsage,
                    docker: Arc::new(docker.clone()),
                },
            )),
            ..Default::default()
        },
        Sensor {
            id: format!("dockercontainer_{}_uptime", container_name),
            name: format!("{} Docker {} Uptime", device_name, container_name),
            icon: Some("mdi:docker".to_string()),
            reporter: Some(SensorReporterType::DockerContainer(
                DockerContainerSensorReporter {
                    container: container.clone(),
                    stats_stash: stats_stash.clone(),
                    stat: DockerContainerSensorReporterStat::Status,
                    docker: Arc::new(docker.clone()),
                },
            )),
            ..Default::default()
        },
    ]
}

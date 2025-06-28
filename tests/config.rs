//! Tests for parsing and generating config TOML (ignoring reporter fields)

use std::collections::HashMap;
use toml;
use unraid_mqtt_stats::config::{
    Config, ConfigDump, DeviceClass, Sensor, SensorConfig, Sensors, SensorsDump,
};

fn example_toml() -> &'static str {
    r#"
[sensors.temp_sensor]
type = "override"
name = "Temperature"
unit = "°C"
device_class = "temperature"
icon = "mdi:thermometer"
disabled = false

[sensors.disk_sensor]
type = "override"
name = "Disk Usage"
unit = "%"
icon = "mdi:harddisk"
disabled = false

[sensors.disk_command]
type = "command"
name = "Disk Usage"
unit = "%"
command = "df" 
args = ["-h", "/mnt/user/appdata"]
icon = "mdi:harddisk"
disabled = false
"#
}

#[test]
fn test_parse_config() {
    let toml_str = example_toml();
    let config: Config = toml::from_str(toml_str).expect("Failed to parse config TOML");
    assert_eq!(config.sensors.len(), 3);

    match &config.sensors["temp_sensor"] {
        Sensors::SensorOverride(sc) => {
            assert_eq!(sc.name.as_deref(), Some("Temperature"));
            assert_eq!(sc.unit.as_deref(), Some("°C"));
            assert_eq!(
                sc.device_class.as_ref().map(|d| format!("{:?}", d)),
                Some("Temperature".to_string())
            );
            assert_eq!(sc.icon.as_deref(), Some("mdi:thermometer"));
            assert!(!sc.disabled);
        }
        _ => panic!("Expected SensorOverride"),
    }
}

#[test]
fn test_generate_config_dump() {
    let mut sensors = HashMap::new();
    sensors.insert(
        "temp_sensor".to_string(),
        SensorsDump::SensorOverride(Sensor {
            id: "temp_sensor".to_string(),
            name: "Temperature".to_string(),
            unit: Some("°C".to_string()),
            device_class: Some(DeviceClass::Temperature),
            icon: Some("mdi:thermometer".to_string()),
            disabled: false,
            reporter: None,
        }),
    );
    let config_dump = ConfigDump { sensors };
    let toml_str = toml::to_string(&config_dump).expect("Failed to serialize ConfigDump");
    assert!(toml_str.contains("temp_sensor"));
    assert!(toml_str.contains("Temperature"));
}

#[test]
fn test_round_trip_config_dump() {
    let toml_str = r#"
[sensors.temp_sensor]
type = "override"
name = "Temperature"
unit = "°C"
device_class = "temperature"
icon = "mdi:thermometer"
disabled = false
"#;
    let config_dump: ConfigDump =
        toml::from_str(toml_str).expect("Failed to parse ConfigDump TOML");
    let toml_out = toml::to_string(&config_dump).expect("Failed to serialize ConfigDump");
    assert!(toml_out.contains("temp_sensor"));
    assert!(toml_out.contains("Temperature"));
}

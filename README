# unraid-mqtt-stats
Gather stats off of unraid server 

# Building on Unraid server
```
git clone .. 
cd unraid-mqtt-stats
docker run --rm -v $(pwd):/workspace -w /workspace rust:latest cargo build

# Better for dev. Caches crates downloads for rebuilds:
mkdir -p ~/.cargo-docker-cache/registry &&  mkdir -p ~/.cargo-docker-cache/git && docker run --rm -v $(pwd):/workspace -v ~/.cargo-docker-cache/registry:/usr/local/cargo/registry -v ~/.cargo-docker-cache/git:/usr/local/cargo/git -w /workspace rust:latest cargo build --release
```

# Usage

### Dump the list of sensors 
./unraid-mqtt-stats --host 192.168.68.0 --device-name arrakis  --sensor-dump sensors.toml

### Dry run, just output the json that would be sent.
./unraid-mqtt-stats --device-name arrakis  -c sensors.toml --json-output

### Basic usage with Home Assistant discovery
./unraid-mqtt-stats --host 192.168.1.100 --username mqtt_user --password mqtt_pass

### Custom device name (useful for multiple Unraid servers)
./unraid-mqtt-stats --device-name arrakis 

### Skip discovery (just update existing sensors)
./unraid-mqtt-stats --skip-discovery

# Custom sensors
You can create custom sensors by creating a config file. Currently sensors just call out to 
commands.  see example_sensors.toml.

You can also over existing sensors by using the `--sensor-dump` option to dump the current sensors to a file, 
then edit that file and use it with the `-c` option.

# Debug
## helps with timing and showing which sensors are running
RUST_LOG=unraid_mqtt_stats=trace RUST_LOG_SPAN_EVENTS=full ./unraid-mqtt-stats --host 192.168.68.0 --device-name arrakis -c sensors.toml --json-output

# Config
# Electra SEMS (Station Energy Management System)

A web server for managing electric vehicle charging station configurations and energy management.

## Features

- Load station configuration from JSON file at startup
- RESTful API for accessing station configuration
- Health check endpoint
- Configurable server port

## Usage

### Running the Server

```bash
cargo run -- --config examples/station_config.json --port 3000
```

### Command Line Arguments

- `--config` or `-c`: Path to the station configuration JSON file (required)
- `--port` or `-p`: Port to bind the server to (default: 3000)

### Example

```bash
# Run with default port (3000)
cargo run -- -c examples/station_config.json

# Run with custom port
cargo run -- -c examples/station_config.json -p 8080
```

## API Endpoints

### Health Check
- **GET** `/health`
- Returns: `OK`
- Description: Simple health check endpoint

### Get Configuration
- **GET** `/config`
- Returns: Current station configuration as JSON
- Description: Retrieves the currently loaded station configuration

## Configuration File Format

The station configuration should be a JSON file with the following structure:

```json
{
  "stationId": "ELECTRA_PARIS_15",
  "gridCapacity": 400,
  "chargers": [
    {
      "id": "CP001",
      "maxPower": 200,
      "connectors": 2
    }
  ],
  "battery": {
    "initialCapacity": 200,
    "power": 100
  }
}
```

### Configuration Fields

- `stationId`: Unique identifier for the charging station
- `gridCapacity`: Maximum grid capacity in kW
- `chargers`: Array of charger configurations
  - `id`: Unique identifier for the charger
  - `maxPower`: Maximum power in kW (shared between connectors)
  - `connectors`: Number of connectors for this charger
- `battery`: Battery system configuration
  - `initialCapacity`: Initial battery capacity in kWh
  - `power`: Maximum charge/discharge power in kW

## Development

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Example Configuration

An example configuration file is provided at `examples/station_config.json`.

## Architecture

- **AppState**: Contains the shared application state with immutable station configuration
- **StationConfig**: Represents the charging station configuration loaded from JSON at startup
- **Web Server**: Axum-based HTTP server with async support
- **Configuration Loading**: Reads and validates JSON configuration at startup (static, no runtime changes)
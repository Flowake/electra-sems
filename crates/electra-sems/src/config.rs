use serde::{Deserialize, Serialize};

/// Represents a charging station configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationConfig {
    pub station_id: String,
    /// Grid capacity in kW
    pub grid_capacity: u32,
    pub chargers: Vec<Charger>,
    pub battery: Battery,
}

/// Represents a charger point within the station
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Charger {
    pub id: String,
    /// Maximum power in kW (shared between connectors)
    pub max_power: u32,
    /// Number of connectors for this charger
    pub connectors: u32,
}

/// Represents the battery system of the station
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    /// Initial capacity in kWh
    pub initial_capacity: u32,
    /// Maximum charge and discharge power in kW
    pub power: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_config_serialization() {
        let config = StationConfig {
            station_id: "ELECTRA_PARIS_15".to_string(),
            grid_capacity: 400,
            chargers: vec![
                Charger {
                    id: "CP001".to_string(),
                    max_power: 200,
                    connectors: 2,
                },
                Charger {
                    id: "CP002".to_string(),
                    max_power: 200,
                    connectors: 2,
                },
                Charger {
                    id: "CP003".to_string(),
                    max_power: 300,
                    connectors: 2,
                },
            ],
            battery: Battery {
                initial_capacity: 200,
                power: 100,
            },
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        println!("{}", json);

        let deserialized: StationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.station_id, deserialized.station_id);
        assert_eq!(config.grid_capacity, deserialized.grid_capacity);
        assert_eq!(config.chargers.len(), deserialized.chargers.len());
    }

    #[test]
    fn test_json_deserialization() {
        let json = r#"
        {
          "stationId": "ELECTRA_PARIS_15",
          "gridCapacity": 400,
          "chargers": [
            {"id": "CP001", "maxPower": 200, "connectors": 2},
            {"id": "CP002", "maxPower": 200, "connectors": 2},
            {"id": "CP003", "maxPower": 300, "connectors": 2}
          ],
          "battery": {
            "initialCapacity": 200,
            "power": 100
          }
        }
        "#;

        let config: StationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.station_id, "ELECTRA_PARIS_15");
        assert_eq!(config.grid_capacity, 400);
        assert_eq!(config.chargers.len(), 3);
        assert_eq!(config.battery.initial_capacity, 200);
        assert_eq!(config.battery.power, 100);
    }
}

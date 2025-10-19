use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationConfig {
    pub station_id: String,
    pub grid_capacity: u32,
    pub chargers: Vec<ChargerConfig>,
    pub battery: Option<Bess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargerConfig {
    pub id: String,
    pub max_power: u32,
    pub connectors: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bess {
    initial_capacity: u32,
    power: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: uuid::Uuid,
    pub connector_id: ConnectorId,
    pub allocated_power: u32,
    pub vehicle_max_power: u32,
}

impl Session {
    pub(crate) fn new(connector_id: ConnectorId, vehicle_max_power: u32) -> Self {
        Session {
            session_id: uuid::Uuid::new_v4(),
            connector_id,
            allocated_power: 0,
            vehicle_max_power,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorId {
    pub charger_id: String,
    pub idx: u8,
}

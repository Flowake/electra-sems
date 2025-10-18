use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationConfig {
    station_id: String,
    grid_capacity: u32,
    chargers: Vec<ChargerConfig>,
    battery: Option<Bess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargerConfig {
    id: String,
    max_power: u32,
    connectors: u8,
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
    session_id: uuid::Uuid,
    connector_id: ConnectorId,
    allocated_power: u32,
    vehicle_max_power: u32,
}

impl Session {
    pub fn new(connector_id: ConnectorId, allocated_power: u32, vehicle_max_power: u32) -> Self {
        Session {
            session_id: uuid::Uuid::new_v4(),
            connector_id,
            allocated_power,
            vehicle_max_power,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorId {
    pub charger_id: String,
    idx: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationState {
    config: StationConfig,
    sessions: HashMap<uuid::Uuid, Session>,
    chargers: HashMap<String, ChargerConfig>,
}

impl StationState {
    pub fn new(config: StationConfig) -> Self {
        let chargers = config
            .chargers
            .iter()
            .map(|charger| (charger.id.clone(), charger.clone()))
            .collect();
        StationState {
            config,
            chargers,
            sessions: HashMap::new(),
        }
    }

    /// Return the total allocated power of the station.
    ///
    /// This is the sum of all allocated power of all sessions.
    pub fn station_allocated_power(&self) -> u32 {
        self.sessions
            .values()
            .map(|session| session.allocated_power)
            .sum()
    }

    /// Return the remaining capacity of the station.
    ///
    /// This is the difference between the grid capacity and the total allocated power.
    pub fn station_remaining_capacity(&self) -> u32 {
        self.config.grid_capacity - self.station_allocated_power()
    }

    /// Return the allocated power of a charger.
    ///
    /// This is the sum of all allocated power of all sessions connected to the charger.
    pub fn charger_allocated_power(&self, charger_id: &str) -> u32 {
        self.sessions
            .values()
            .filter(|session| session.connector_id.charger_id == charger_id)
            .map(|session| session.allocated_power)
            .sum()
    }

    /// Return the remaining capacity of a charger.
    ///
    /// This is the difference between the maximum power of the charger and the total allocated power of all sessions connected to the charger.
    ///
    /// Note: This cannot exceed the remaining capacity of the station.
    pub fn charger_remaining_capacity(&self, charger_id: &str) -> u32 {
        let station_remaining_capacity = self.station_remaining_capacity();
        self.chargers
            .get(charger_id)
            .map_or(0, |charger| {
                charger.max_power
                    - self
                        .sessions
                        .values()
                        .filter(|session| session.connector_id.charger_id == charger_id)
                        .map(|session| session.allocated_power)
                        .sum::<u32>()
            })
            .min(station_remaining_capacity)
    }

    pub fn start_session(&mut self, connector_id: ConnectorId, vehicle_max_power: u32) -> Session {
        let charger_remaining_capacity = self
            .charger_remaining_capacity(&connector_id.charger_id)
            .min(vehicle_max_power);
        let new_session = Session::new(connector_id, charger_remaining_capacity, vehicle_max_power);
        self.sessions
            .insert(new_session.session_id, new_session.clone());
        new_session
    }

    pub fn stop_session(&mut self, session_id: uuid::Uuid) {
        self.sessions.remove(&session_id);
    }

    /// Update the power allocation for a session.
    ///
    /// The allocated power is updated to the new value, while ensuring
    /// that the power allocation does not exceed the remaining capacity of the charger
    /// and the station's total capacity.
    pub fn power_update(&mut self, session_id: uuid::Uuid, power: u32) -> Session {
        let new_allocated_power = {
            let session = self.sessions.get(&session_id).expect("Session not found");

            power.min(self.charger_remaining_capacity(&session.connector_id.charger_id))
        };
        let session = self
            .sessions
            .get_mut(&session_id)
            .expect("Session not found");
        session.allocated_power = new_allocated_power;
        session.clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn default_config() -> StationConfig {
        StationConfig {
            station_id: "ELECTRA_PARIS_15".into(),
            grid_capacity: 400,
            chargers: vec![
                ChargerConfig {
                    id: "CP001".into(),
                    max_power: 200,
                    connectors: 2,
                },
                ChargerConfig {
                    id: "CP002".into(),
                    max_power: 200,
                    connectors: 2,
                },
                ChargerConfig {
                    id: "CP003".into(),
                    max_power: 300,
                    connectors: 2,
                },
            ],
            battery: None,
        }
    }

    fn default_state() -> StationState {
        StationState::new(default_config())
    }

    #[test]
    fn test_charger_capacity() {
        let mut state = default_state();
        let session_1 = state.start_session(
            ConnectorId {
                charger_id: "CP001".into(),
                idx: 1,
            },
            100,
        );
        assert_eq!(
            session_1.allocated_power, 100,
            "Session power allocation should match vehicle max power"
        );
        let session_2 = state.start_session(
            ConnectorId {
                charger_id: "CP001".into(),
                idx: 1,
            },
            150,
        );
        assert_eq!(
            session_2.allocated_power, 100,
            "Session power allocation should be limited by charger capacity"
        );
    }

    #[test]
    fn test_station_capacity() {
        let mut state = default_state();
        let session_1 = state.start_session(
            ConnectorId {
                charger_id: "CP001".into(),
                idx: 1,
            },
            200,
        );
        assert_eq!(
            session_1.allocated_power, 200,
            "Session power allocation should match vehicle max power"
        );
        let session_2 = state.start_session(
            ConnectorId {
                charger_id: "CP003".into(),
                idx: 1,
            },
            300,
        );
        assert_eq!(
            session_2.allocated_power, 200,
            "Session power allocation should be limited by charger capacity"
        );
    }
}

fn allocate_connector(sessions: &[Session], charger_capacity: u32) -> Vec<Session> {
    let mut out_sessions: Vec<Session> = sessions
        .iter()
        .map(|s| {
            let mut out = s.clone();
            out.allocated_power = 0;
            out
        })
        .collect();

    loop {
        // We compute the remaning power available for the connector
        let remaining_power =
            charger_capacity - out_sessions.iter().map(|s| s.allocated_power).sum::<u32>();
        // We count the number of vehicles which could take the remaining power
        let sessions_with_remaining_power = out_sessions
            .iter()
            .filter(|s| s.allocated_power < s.vehicle_max_power)
            .count();
        // If we have no power to share or no vehicles left to take the remaining power,
        // the sharing loop is complete
        if remaining_power == 0 || sessions_with_remaining_power == 0 {
            break;
        }
        // We split the remaining power between the vehicles
        let fair_share = remaining_power as usize / sessions_with_remaining_power;
        // We attribute
        out_sessions
            .iter_mut()
            .filter(|s| s.allocated_power < s.vehicle_max_power)
            .for_each(|s| {
                s.allocated_power =
                    (s.allocated_power + fair_share as u32).min(s.vehicle_max_power);
            });
    }
    out_sessions
}

#[cfg(test)]
mod test_allocate_connector {
    use super::*;

    fn assert_eq_allocated_power(
        before: &Session,
        after_sessions: &[Session],
        allocated_power: u32,
    ) {
        assert_eq!(
            after_sessions
                .iter()
                .find(|s| s.session_id == before.session_id)
                .expect("Could not find section")
                .allocated_power,
            allocated_power
        )
    }

    fn get_sessions() -> Vec<Session> {
        vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                0,
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                0,
                150,
            ),
        ]
    }

    #[test]
    /// Low capacity available, no vehicle at full power
    fn test_no_max_power() {
        let sessions = get_sessions();
        let out_sessions = allocate_connector(&sessions, 100);
        assert_eq_allocated_power(&sessions[0], &out_sessions, 50);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 50);
    }

    #[test]
    /// Low capacity available, one vehicle at max power
    fn test_partial_max_power() {
        let sessions = get_sessions();
        let out_sessions = allocate_connector(&sessions, 200);
        assert_eq_allocated_power(&sessions[0], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 100);
    }

    #[test]
    /// High capacity available, all vehicle at max power
    fn test_all_max_power() {
        let sessions = get_sessions();
        let out_sessions = allocate_connector(&sessions, 250);
        assert_eq_allocated_power(&sessions[0], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 150);
    }

    #[test]
    /// High capacity available, all vehicle at max power, remaining capacity
    fn test_all_max_power_remaining() {
        let sessions = get_sessions();
        let out_sessions = allocate_connector(&sessions, 300);
        assert_eq_allocated_power(&sessions[0], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 150);
    }
}

// fn allocate_power_station(
//     sessions: &HashMap<uuid::Uuid, Session>,
//     chargers: &HashMap<String, ChargerConfig>,
//     station_capacity: u32,
// ) {
//     let total_max_power: u32 = sessions
//         .values()
//         .map(|session| session.vehicle_max_power)
//         .sum();

//     // We can allocate the maximum power
//     if total_max_power <= station_capacity {}
// }

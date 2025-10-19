use std::collections::HashMap;

use crate::{ChargerConfig, Session};

fn allocate_power_station(
    current_sessions: &HashMap<uuid::Uuid, Session>,
    chargers_config: &HashMap<String, ChargerConfig>,
    station_capacity: u32,
) -> HashMap<uuid::Uuid, Session> {
    // Split the sessions based on their charger.
    let mut chargers_sessions: HashMap<String, Vec<Session>> = chargers_config
        .iter()
        .filter_map(|(k, v)| {
            let charger_sessions = current_sessions
                .values()
                .filter(|s| s.connector_id.charger_id == v.id)
                .map(|s| {
                    let mut out = s.clone();
                    out.allocated_power = 0;
                    out
                })
                .collect::<Vec<_>>();
            if charger_sessions.is_empty() {
                None
            } else {
                Some((k.clone(), charger_sessions))
            }
        })
        .collect();

    loop {
        // We compute the remaning power available for the station
        let remaining_power = station_capacity.saturating_sub(
            chargers_sessions
                .values()
                .map(|sessions| sessions.iter().map(|s| s.allocated_power).sum::<u32>())
                .sum(),
        );
        // If we don't have any remaining power to share, stop the loop
        if remaining_power == 0 {
            break;
        }
        // The chargers and their associated sessions that can still take more power
        let chargers_with_remaining_power: HashMap<String, Vec<Session>> = chargers_sessions
            .iter()
            // We only keep the chargers that are not at their max power
            .filter(|(charger_id, sessions)| {
                let vehicles_allocated = sessions.iter().map(|s| s.allocated_power).sum::<u32>();
                let charger_capacity = chargers_config
                    .get(*charger_id)
                    .map(|c| c.max_power)
                    .unwrap_or(0);
                vehicles_allocated < charger_capacity
            })
            // Then we keep the chargers whose vehicles can take more power
            .filter(|(_, sessions)| {
                sessions
                    .iter()
                    .filter(|s| s.allocated_power < s.vehicle_max_power)
                    .count()
                    > 0
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // If no charger can take more power, we stop the loop
        if chargers_with_remaining_power.is_empty() {
            break;
        }
        // For each charger, the number of sessions that can still take
        // more power
        let sessions_with_remaining_power: HashMap<String, usize> = chargers_with_remaining_power
            .iter()
            .map(|(charger_id, sessions)| {
                (
                    charger_id.clone(),
                    sessions
                        .iter()
                        .filter(|s| s.allocated_power < s.vehicle_max_power)
                        .count(),
                )
            })
            .filter(|(_k, v)| *v > 0)
            .collect();
        // We split the remaining power between the vehicles
        let fair_share =
            remaining_power / sessions_with_remaining_power.values().sum::<usize>() as u32;
        // We share the power between the stations that can still take power
        for (charger_id, charger_sessions) in chargers_sessions
            .iter_mut()
            .filter(|(charger_id, _)| sessions_with_remaining_power.contains_key(*charger_id))
        {
            let sessions_with_remaining_power_for_charger = *sessions_with_remaining_power
                .get(charger_id)
                .expect("We filtered the HashMap, so this key should exist")
                as u32;
            let additional_power = sessions_with_remaining_power_for_charger * fair_share;
            let current_allocated_power: u32 =
                charger_sessions.iter().map(|s| s.allocated_power).sum();
            let power_to_allocate = (additional_power as u32).min(
                chargers_config
                    .get(charger_id)
                    .expect("Charger config not found")
                    .max_power
                    - current_allocated_power,
            );
            charger_sessions
                .iter_mut()
                .filter(|session| session.allocated_power < session.vehicle_max_power)
                .for_each(|session| {
                    session.allocated_power = (session.allocated_power
                        + power_to_allocate / sessions_with_remaining_power_for_charger)
                        .min(session.vehicle_max_power)
                })
        }
    }
    chargers_sessions
        .values()
        .flat_map(|sessions| sessions.iter().map(|s| (s.session_id, s.clone())))
        .collect()
}

#[cfg(test)]
mod test_allocate_station {
    use super::*;
    use crate::ConnectorId;

    #[track_caller]
    fn assert_eq_allocated_power(
        before: &Session,
        after_sessions: &HashMap<uuid::Uuid, Session>,
        allocated_power: u32,
    ) {
        assert_eq!(
            after_sessions
                .get(&before.session_id)
                .expect("Could not find section")
                .allocated_power,
            allocated_power
        )
    }

    fn vec_session_to_hashmap(sessions: &[Session]) -> HashMap<uuid::Uuid, Session> {
        sessions
            .iter()
            .map(|session| (session.session_id, session.clone()))
            .collect()
    }

    fn vec_chargers_to_hashmap(chargers: &[ChargerConfig]) -> HashMap<String, ChargerConfig> {
        chargers
            .iter()
            .map(|charger| (charger.id.clone(), charger.clone()))
            .collect()
    }

    #[test]
    fn test_no_chargers_limit() {
        let sessions = vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                200,
            ),
        ];
        let chargers_config = vec_chargers_to_hashmap(&[
            ChargerConfig {
                id: "CP001".to_string(),
                max_power: 300,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP002".to_string(),
                max_power: 300,
                connectors: 2,
            },
        ]);

        let out_sessions =
            allocate_power_station(&vec_session_to_hashmap(&sessions), &chargers_config, 1000);

        // We expect every vehicle to be at max power
        assert_eq_allocated_power(&sessions[0], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[2], &out_sessions, 200);
    }

    #[test]
    fn test_chargers_limit() {
        let sessions = vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                200,
            ),
        ];
        let chargers_config = vec_chargers_to_hashmap(&[
            ChargerConfig {
                id: "CP001".to_string(),
                max_power: 100,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP002".to_string(),
                max_power: 100,
                connectors: 2,
            },
        ]);

        let out_sessions =
            allocate_power_station(&vec_session_to_hashmap(&sessions), &chargers_config, 500);

        // We expect the first charger to be at max power, and all the remaining power
        // to go to the second charger.
        assert_eq_allocated_power(&sessions[0], &out_sessions, 50);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 50);
        assert_eq_allocated_power(&sessions[2], &out_sessions, 100);
    }

    #[test]
    fn test_station_limit() {
        let sessions = vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                200,
            ),
        ];
        let chargers_config = vec_chargers_to_hashmap(&[
            ChargerConfig {
                id: "CP001".to_string(),
                max_power: 300,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP002".to_string(),
                max_power: 300,
                connectors: 2,
            },
        ]);

        let out_sessions =
            allocate_power_station(&vec_session_to_hashmap(&sessions), &chargers_config, 300);

        // The 3 vehicles should take a third each, as it is below their max power
        assert_eq_allocated_power(&sessions[0], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[2], &out_sessions, 100);
    }

    #[test]
    fn test_station_limit_and_vehicles() {
        let sessions = vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                50,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                200,
            ),
        ];
        let chargers_config = vec_chargers_to_hashmap(&[
            ChargerConfig {
                id: "CP001".to_string(),
                max_power: 300,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP002".to_string(),
                max_power: 300,
                connectors: 2,
            },
        ]);

        let out_sessions =
            allocate_power_station(&vec_session_to_hashmap(&sessions), &chargers_config, 300);

        // The first charger should be at max power, and the second one taking the rest
        assert_eq_allocated_power(&sessions[0], &out_sessions, 50);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[2], &out_sessions, 150);
    }

    #[test]
    fn test_empty_stations() {
        let sessions = vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                100,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                100,
            ),
        ];
        let chargers_config = vec_chargers_to_hashmap(&[
            ChargerConfig {
                id: "CP001".to_string(),
                max_power: 300,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP002".to_string(),
                max_power: 300,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP003".to_string(),
                max_power: 300,
                connectors: 2,
            },
        ]);

        let out_sessions =
            allocate_power_station(&vec_session_to_hashmap(&sessions), &chargers_config, 300);

        assert_eq_allocated_power(&sessions[0], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 100);
        assert_eq_allocated_power(&sessions[2], &out_sessions, 100);
    }

    #[test]
    fn test_fairness_accross_chargers() {
        let sessions = vec![
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                80,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                150,
            ),
            Session::new(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                150,
            ),
        ];
        let chargers_config = vec_chargers_to_hashmap(&[
            ChargerConfig {
                id: "CP001".to_string(),
                max_power: 200,
                connectors: 2,
            },
            ChargerConfig {
                id: "CP002".to_string(),
                max_power: 200,
                connectors: 2,
            },
        ]);

        let out_sessions =
            allocate_power_station(&vec_session_to_hashmap(&sessions), &chargers_config, 330);

        assert_eq_allocated_power(&sessions[0], &out_sessions, 80);
        assert_eq_allocated_power(&sessions[1], &out_sessions, 120);
        assert_eq_allocated_power(&sessions[2], &out_sessions, 130);
    }
}

pub(crate) fn allocate_for_new_session(
    mut sessions: HashMap<uuid::Uuid, Session>,
    chargers_config: &HashMap<String, ChargerConfig>,
    grid_capacity: u32,
    hardcap_capacity: u32,
    new_session: &Session,
) -> Session {
    sessions.insert(new_session.session_id, new_session.clone());

    let mut reallocated_sessions =
        allocate_power_station(&sessions, chargers_config, grid_capacity);
    let mut new_allocated_session = reallocated_sessions
        .remove_entry(&new_session.session_id)
        .expect("Could not find allocated session")
        .1;

    // The reallocation might lower the power for other sessions, but it will not be effective
    // immediately (not until their next call to power_update). As such we need to ensure that
    // we do not exceed the hardcap capacity.
    new_allocated_session.allocated_power =
        new_allocated_session.allocated_power.min(hardcap_capacity);
    new_allocated_session
}

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use sems_core::{ConnectorId, Session, SessionError, StationState};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub connector_id: ConnectorId,
    pub vehicle_max_power: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub session: Session,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerUpdateRequest {
    pub consumed_power: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
}

fn session_error_to_response(error: SessionError) -> impl IntoResponse {
    let (status, message) = match error {
        SessionError::ConnectorAlreadyInUse { connector_id } => (
            StatusCode::CONFLICT,
            format!(
                "Connector {}:{} is already in use",
                connector_id.charger_id, connector_id.idx
            ),
        ),
        SessionError::ConnectorNotFound { connector_id } => (
            StatusCode::NOT_FOUND,
            format!(
                "Connector {}:{} not found",
                connector_id.charger_id, connector_id.idx
            ),
        ),
        SessionError::SessionNotFound { session_id } => (
            StatusCode::NOT_FOUND,
            format!("Session {} not found", session_id),
        ),
    };

    (status, Json(ErrorResponse { error: message }))
}

/// Create a new charging session
pub async fn create_session(
    State(app_state): State<Arc<Mutex<StationState>>>,
    Json(payload): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let mut state = app_state.lock().unwrap();
    match state.start_session(payload.connector_id, payload.vehicle_max_power) {
        Ok(session) => (StatusCode::OK, Json(SessionResponse { session })).into_response(),
        Err(error) => session_error_to_response(error).into_response(),
    }
}

/// Stop an existing charging session
pub async fn stop_session(
    State(app_state): State<Arc<Mutex<StationState>>>,
    Path(session_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut state = app_state.lock().unwrap();
    state.stop_session(session_id);
    StatusCode::NO_CONTENT
}

/// Update the power consumption for an existing session
pub async fn power_update(
    State(app_state): State<Arc<Mutex<StationState>>>,
    Path(session_id): Path<Uuid>,
    Json(payload): Json<PowerUpdateRequest>,
) -> impl IntoResponse {
    let mut state = app_state.lock().unwrap();
    match state.power_update(session_id, payload.consumed_power) {
        Ok(session) => (StatusCode::OK, Json(SessionResponse { session })).into_response(),
        Err(error) => session_error_to_response(error).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::post};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sems_core::{ChargerConfig, StationConfig};
    use tower::util::ServiceExt;

    /// Create the application router with session endpoints
    pub fn create_app(app_state: StationState) -> Router {
        let shared_state = Arc::new(Mutex::new(app_state));
        Router::new()
            .route("/sessions", post(create_session))
            .route("/sessions/{session_id}/stop", post(stop_session))
            .route("/sessions/{session_id}/power-update", post(power_update))
            .with_state(shared_state)
    }

    fn test_station_config() -> StationConfig {
        StationConfig {
            station_id: "TEST_STATION".into(),
            grid_capacity: 400,
            chargers: vec![
                ChargerConfig {
                    id: "CP001".into(),
                    max_power: 200,
                    connectors: 2,
                },
                ChargerConfig {
                    id: "CP002".into(),
                    max_power: 150,
                    connectors: 1,
                },
            ],
            battery: None,
        }
    }

    #[tokio::test]
    async fn test_create_session() {
        let config = test_station_config();
        let state = StationState::new(config);
        let app = create_app(state);

        let create_request = CreateSessionRequest {
            connector_id: ConnectorId {
                charger_id: "CP001".to_string(),
                idx: 1,
            },
            vehicle_max_power: 150,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/sessions")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let session_response: SessionResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(session_response.session.connector_id.charger_id, "CP001");
        assert_eq!(session_response.session.connector_id.idx, 1);
        assert_eq!(session_response.session.vehicle_max_power, 150);
        assert!(session_response.session.allocated_power > 0);
    }

    #[tokio::test]
    async fn test_create_session_connector_not_found() {
        let config = test_station_config();
        let state = StationState::new(config);
        let app = create_app(state);

        let create_request = CreateSessionRequest {
            connector_id: ConnectorId {
                charger_id: "CP999".to_string(), // Non-existent charger
                idx: 1,
            },
            vehicle_max_power: 150,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/sessions")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert!(error_response.error.contains("Connector CP999:1 not found"));
    }

    #[tokio::test]
    async fn test_create_session_connector_already_in_use() {
        let config = test_station_config();
        let mut state = StationState::new(config);

        // Start a session first
        let connector_id = ConnectorId {
            charger_id: "CP001".to_string(),
            idx: 1,
        };
        state.start_session(connector_id.clone(), 100).unwrap();

        let app = create_app(state);

        let create_request = CreateSessionRequest {
            connector_id,
            vehicle_max_power: 150,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/sessions")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert!(error_response.error.contains("is already in use"));
    }

    #[tokio::test]
    async fn test_stop_session() {
        let config = test_station_config();
        let mut state = StationState::new(config);

        // Start a session first
        let connector_id = ConnectorId {
            charger_id: "CP001".to_string(),
            idx: 1,
        };
        let session = state.start_session(connector_id, 150).unwrap();
        let session_id = session.session_id;

        let app = create_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/sessions/{}/stop", session_id))
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_power_update() {
        let config = test_station_config();
        let mut state = StationState::new(config);

        // Start a session first
        let connector_id = ConnectorId {
            charger_id: "CP001".to_string(),
            idx: 1,
        };
        let session = state.start_session(connector_id, 150).unwrap();
        let session_id = session.session_id;

        let app = create_app(state);

        let power_update_request = PowerUpdateRequest {
            consumed_power: 100,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/sessions/{}/power-update", session_id))
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&power_update_request).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let session_response: SessionResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(session_response.session.session_id, session_id);
        // The vehicle_max_power should be updated to the consumed_power if it's less
        assert_eq!(session_response.session.vehicle_max_power, 100);
    }

    #[tokio::test]
    async fn test_power_update_session_not_found() {
        let config = test_station_config();
        let state = StationState::new(config);
        let app = create_app(state);

        let fake_session_id = Uuid::new_v4();
        let power_update_request = PowerUpdateRequest {
            consumed_power: 100,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/sessions/{}/power-update", fake_session_id))
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&power_update_request).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert!(
            error_response.error.contains("Session") && error_response.error.contains("not found")
        );
    }
}

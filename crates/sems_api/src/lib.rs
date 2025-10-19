//! SEMS API Library
//!
//! This library provides the HTTP API for the Station Energy Management System.

mod station;

use axum::{Router, routing::get};
use sems_core::StationState;

/// Health check endpoint
pub async fn health_check() -> &'static str {
    "OK"
}

/// Create the application router with all endpoints
pub fn create_app(app_state: StationState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/station/config", get(station::get_station_config))
        .route("/station/status", get(station::get_station_status))
        .with_state(app_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::util::ServiceExt;

    pub fn create_test_app() -> Router {
        Router::new().route("/health", get(health_check))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}

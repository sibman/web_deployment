use axum::{routing::get, body::Body, http::{Response, StatusCode}, Router,};
use std::{sync::{Arc, Mutex}, collections::HashMap};
use axum::response::IntoResponse;
use serde_json::json;
use std::fmt::Debug;

// Define a trait for health checkers
pub trait HealthChecker: Send + Sync + Debug {
    fn is_ready(&self) -> bool;
    fn is_alive(&self) -> bool;
}

// Actuator struct to manage health checkers and routes
pub struct Actuator {
    health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>,
}

impl Actuator {
    // Create a new Actuator instance
    pub fn new() -> Self {
        Self {
            health_checkers: Arc::new(HashMap::new()),
        }
    }

    // Add a health checker
    pub fn add_health_checker(&mut self, name: String, checker: Arc<Mutex<dyn HealthChecker>>) {
        let health_checkers = Arc::get_mut(&mut self.health_checkers).unwrap();
        health_checkers.insert(name, checker);
    }

    pub fn get_health_checkers(&self) -> &Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>> {
        &self.health_checkers
    }

    // Generate the actuator router
    pub fn router(&self, router: Router) -> Router {
        let health_checkers_readiness = self.health_checkers.clone();
        let health_checkers_liveness = self.health_checkers.clone();
        let health_checkers_info = self.health_checkers.clone();
        let health_checkers_health = self.health_checkers.clone();
        // Create a router with /actuator/health/readiness, /actuator/health/liveness, /actuator/info, and /actuator/health endpoints
        router
            .route("/actuator/health/readiness", get(|| async move {                
                readiness_handler(health_checkers_readiness).await // Call the readiness_handler method within the closure
            }))
            .route("/actuator/health/liveness", get(|| async move {
                liveness_handler(health_checkers_liveness).await // Call the liveness_handler method within the closure
            }))
            .route("/actuator/info", get(|| async move {
                info_handler(health_checkers_info).await // Call the info_handler method within the closure
            }))
            .route("/actuator/health", get(|| async move {
                health_handler(health_checkers_health).await // Call the health_handler method within the closure
            }))
    }
}

// Handler for /actuator/info endpoint
async fn info_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_ready = check_all_health(health_checkers.clone(), |checker| checker.is_ready()).await;
    let is_alive = check_all_health(health_checkers.clone(), |checker| checker.is_alive()).await;

    Response::builder()
        .status(if is_ready && is_alive { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

// Placeholder health handler function
async fn health_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_ready = check_all_health(health_checkers.clone(), |checker| checker.is_ready()).await;
    let is_alive = check_all_health(health_checkers.clone(), |checker| checker.is_alive()).await;

    let status = if is_ready && is_alive {
        "UP"
    } else {
        "DOWN"
    };

    Response::builder()
        .status(if is_ready && is_alive { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(json!({ "status": status }).to_string())
        .unwrap()
}

// Handler for /actuator/health/readiness endpoint
async fn readiness_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_ready = check_all_health(health_checkers.clone(), |checker| checker.is_ready()).await;

    Response::builder()
        .status(if is_ready { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(json!({ "status": if is_ready { "UP" } else { "DOWN" } }).to_string())
        .unwrap()
}

// Handler for /actuator/health/liveness endpoint
async fn liveness_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_alive = check_all_health(health_checkers.clone(), |checker| checker.is_alive()).await;

    Response::builder()
        .status(if is_alive { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(json!({ "status": if is_alive { "UP" } else { "DOWN" } }).to_string())
        .unwrap()
}

// Helper function to check all health checkers
async fn check_all_health<F>(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>, check_fn: F) -> bool
where
    F: Fn(&dyn HealthChecker) -> bool,
{
    let mut is_health = true;
    for (_, checker) in health_checkers.iter() {
        let checker = checker.lock().unwrap();
        if !check_fn(&*checker) {
            is_health = false;
            break;
        }
    }
    is_health
}

// Example health check function
// pub fn database_health_check() -> bool {
//     // Placeholder implementation for database health check
//     true
// }

// fn main() {
//     // Create a new Actuator instance
//     let mut actuator = Actuator::new();

//     // Add health checkers
//     actuator.add_health_checker("database".to_string(), Arc::new(Mutex::new(database_health_check)));

//     // Generate the actuator router
//     let router = actuator.router();

//     // Start the server with the actuator routes
//     // axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
//     //     .serve(router.into_make_service())
//     //     .await
//     //     .unwrap();
// }

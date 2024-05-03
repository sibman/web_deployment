pub mod api {
    use axum::response::IntoResponse;
    use axum::{
        body::Body,
        http::{Response, StatusCode},
        routing::get,
        Router,
    };
    use serde_json::json;
    use std::fmt::Debug;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    use std::future::Future;
    use std::pin::Pin;
    use lazy_static::lazy_static;
    use std::sync::MutexGuard;
    use std::ops::DerefMut;
    use axum::extract::Extension;

    // Define a trait for health checkers
    pub trait StateChecker: Send + Sync + Debug {
        fn is_ready(&self) -> bool;
        fn is_alive(&self) -> bool;
    }

    type ActuatorStateDb = Arc<HashMap<String, Arc<Mutex<Box<dyn StateChecker>>>>>;
    //type HandlerFn = fn(state: &ActuatorStateDb) -> Result<Response<String>, Box<dyn std::error::Error>>;

    pub trait ActuatorRouter: Send + Sync + Debug {
        fn register_routes_with_extention(
            //self: &Self, 
            router: Router, 
            //handler_map: HashMap<String, HandlerFn>,
            extention: Option<Extension<ActuatorState>>) -> Router;
    } 

    //Handler for /actuator/info endpoint
    async fn info_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_ready = check_all_health(&state.health_checkers, |checker| checker.is_ready()).await;
        let is_alive = check_all_health(&state.health_checkers, |checker| checker.is_alive()).await;

        Response::builder()
            .status(if is_ready && is_alive {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            })
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .unwrap()
    }

    // Placeholder health handler function
    async fn health_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_ready = check_all_health(&state.health_checkers, |checker| checker.is_ready()).await;
        let is_alive = check_all_health(&state.health_checkers, |checker| checker.is_alive()).await;
        let status = if is_ready && is_alive { "UP" } else { "DOWN" };

        Response::builder()
            .status(if is_ready && is_alive {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            })
            .header("Content-Type", "application/json")
            .body(json!({ "status": status }).to_string())
            .unwrap()
    }

    // Handler for /actuator/health/readiness endpoint
    async fn readiness_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_ready = check_all_health(&state.health_checkers, |checker| checker.is_ready()).await;
        let body = json!({ "status": if is_ready { "UP" } else { "DOWN" } });

        Response::builder()
            .status(if is_ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE })
            .body(body.to_string())
            .unwrap()
    }

    // Handler for /actuator/health/liveness endpoint
    async fn liveness_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_alive = check_all_health(&state.health_checkers, |checker| checker.is_alive()).await;
        let body = json!({ "status": if is_alive { "UP" } else { "DOWN" } });

        Response::builder()
            .status(if is_alive { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE })
            .body(body.to_string())
            .unwrap()
    }
    
    async fn check_all_health<F>(health_checkers: &Arc<HashMap<String, Arc<Mutex<Box<dyn StateChecker>>>>>, check_fn: F) -> bool
    where
        F: Fn(&dyn StateChecker) -> bool,
    {
        let mut is_health = true;
        for (_, checker) in health_checkers.iter() {
            let checker = checker.lock().unwrap();
            if !check_fn(&**checker) {
                is_health = false;
                break;
            }
        }
        is_health
    }

    // ActuatorState struct to manage health checkers and routes
    #[derive(Debug, Default, Clone)]
    pub struct ActuatorState {
        health_checkers: ActuatorStateDb, // Arc<HashMap<String, Arc<Mutex<dyn StateChecker>>>>,
    }

    impl ActuatorState {
        // Create a new ActuatorState instance
        pub fn new() -> Self {
            Self {
                health_checkers: Arc::new(HashMap::new()),
            }
        }

        // Add a health checker
        pub fn add_health_checker(&mut self, name: String, checker: Arc<Mutex<Box<dyn StateChecker>>>) {
            let health_checkers = Arc::get_mut(&mut self.health_checkers).unwrap();
            health_checkers.insert(name, checker);
        }

        // Get a mutable reference to the health_checkers
        pub fn get_mut_health_checkers(&mut self) -> &mut ActuatorStateDb {
            &mut self.health_checkers
        }
    }

    #[derive(Debug)]
    pub struct ActuatorRouterBuilder;
    impl ActuatorRouter for ActuatorRouterBuilder {
        // Generate the actuator router
        fn register_routes_with_extention(
            router: Router, 
            //handler_map: HashMap<String, HandlerFn>,
            extention: Option<Extension<ActuatorState>>) -> Router {
            // Create a router with /actuator/health/readiness, /actuator/health/liveness, /actuator/info, and /actuator/health endpoints
            let mut router = router
                .route(
                    "/actuator/health/readiness", get(readiness_handler),
                )
                .route(
                    "/actuator/health/liveness", get(liveness_handler),
                )
                .route(
                    "/actuator/info", get(info_handler),
                )
                .route(
                    "/actuator/health", get(health_handler), // Call the health_handler method within the closure                    
                );

            if let Some(extention) = extention {
                router = router.layer(extention);
            }

            router
        }
    }

    // impl ActuatorRouter for ActuatorState {
    //     fn register_routes_with_extention(
    //         self: &Self, 
    //         router: Router, 
    //         handler_map: HashMap<String, HandlerFn>,
    //         extention: Option<Extension<ActuatorStateDb>>) -> Router {
    //         let mut router = router;
    //         for (route, handler) in handler_map.iter() {
    //             router = router.route(route, get(handler));                
    //         }
    //         if let Some(extention) = extention {
    //             router = router.layer(extention);
    //         }
    //         router
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::connect_info::MockConnectInfo,
        extract::{ConnectInfo, Extension},
        http::{self, Request, StatusCode},
        routing::{get, post},
        Json,
        Router,
    };
    use http_body_util::BodyExt; // for `collect`
    use serde_json::{json, Value};
    use std::net::SocketAddr;

    use api::{StateChecker, ActuatorState, ActuatorRouter, ActuatorRouterBuilder};
    use http::Method;
    use std::sync::{Arc, Mutex};
    use tower::{Service, ServiceExt}; // for `call`, `oneshot`, and `ready`
    
    pub fn app() -> Router {
        // Compose the routes
        Router::new()
            .route(
                "/json",
                post(|payload: Json<serde_json::Value>| async move {
                    Json(serde_json::json!({ "data": payload.0 }))
                }),
            )
            .route(
                "/requires-connect-info",
                get(|ConnectInfo(addr): ConnectInfo<SocketAddr>| async move { format!("Hi {addr}") }),
            )
        // Add middleware to all routes
        // .layer(
        // )
    }

    #[tokio::test]
    async fn json() {
        let app = app();

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri("/json")
                    .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .body(Body::from(
                        serde_json::to_vec(&json!([1, 2, 3, 4])).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, json!({ "data": [1, 2, 3, 4] }));
    }

    // Here we're calling `/requires-connect-info` which requires `ConnectInfo`
    //
    // That is normally set with `Router::into_make_service_with_connect_info` but we can't easily
    // use that during tests. The solution is instead to set the `MockConnectInfo` layer during
    // tests.
    #[tokio::test]
    async fn with_into_make_service_with_connect_info() {
        let mut app = app()
            .layer(MockConnectInfo(SocketAddr::from(([0, 0, 0, 0], 3000))))
            .into_service();

        let request = Request::builder()
            .uri("/requires-connect-info")
            .body(Body::empty())
            .unwrap();
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[derive(Debug)]
    struct DatabaseHealthCheck {
        ready: bool,
        alive: bool,
    }

    impl StateChecker for DatabaseHealthCheck {
        fn is_ready(&self) -> bool {
            self.ready
        }

        fn is_alive(&self) -> bool {
            self.alive
        }
    }

    #[tokio::test]
    async fn test_actuator() {
        let _app = app();
        let mut actuator = api::ActuatorState::new();
                
        // Add health checkers
        actuator.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        println!("{:?}", actuator);
        //let extention: Option<Extension<ActuatorState>> = Some(Extension(actuator));
        //let app = ActuatorRouterBuilder::register_routes_with_extention(_app, extention);

        actuator.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: false,
                alive: false,
            }))),
        );

        println!("{:?}", actuator);

        //let _app = actuator.route(_app);

        actuator.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        println!("{:?}", actuator);
    }

    #[tokio::test]
    async fn inject_actuator() {
        let app = app();
        // Create a new ActuatorState instance
        let mut actuatorState = api::ActuatorState::new();
        
        // Add health checkers
        actuatorState.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        // Generate the actuator router
        let extention: Option<Extension<ActuatorState>> = Some(Extension(actuatorState));
        let mut app = ActuatorRouterBuilder::register_routes_with_extention(app, extention).into_service();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/health")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/info")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/health/liveness")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/health/readiness")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Add health checkers
        // actuator.add_health_checker("database".to_string(), Arc::new(Mutex::new(DatabaseHealthCheck{ready: false, alive: false})));

        // println!("{:?}", actuator);

        // let request = Request::builder()
        // .method(Method::GET)
        // .uri("/actuator/health")
        // .body(Body::empty())
        // .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // let request = Request::builder()
        //     .method(Method::GET)
        //     .uri("/actuator/info")
        //     .body(Body::empty())
        //     .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // let request = Request::builder()
        //     .method(Method::GET)
        //     .uri("/actuator/health/liveness")
        //     .body(Body::empty())
        //     .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // let request = Request::builder()
        //     .method(Method::GET)
        //     .uri("/actuator/health/readiness")
        //     .body(Body::empty())
        //     .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

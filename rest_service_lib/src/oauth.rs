mod oauth {
    use axum::extract::{cookie::Key, FromRef};
    use axum::{routing::get, Router};
    use reqwest::Client as ReqwestClient;
    use sqlx::PgPool;

    #[derive(Clone)]
    pub struct AppState {
        db: PgPool,
        ctx: ReqwestClient,
        key: Key,
    }

    // implementing FromRef is required here so we can extract substate in Axum
    // read more here: https://docs.rs/axum/latest/axum/extract/trait.FromRef.html
    impl FromRef<AppState> for Key {
        fn from_ref(state: &AppState) -> Self {
            state.key.clone()
        }
    }

    async fn hello_world() -> &'static str {
        "Hello world!"
    }

    #[shuttle_runtime::main]
    async fn axum(
        #[shuttle_shared_db::Postgres] db: PgPool,
        #[shuttle_secrets::Secrets] secrets: SecretStore,
    ) -> shuttle_axum::ShuttleAxum {
        sqlx::migrate!()
            .run(&db)
            .await
            .expect("Failed migrations :(");

        // Getting secrets from our SecretsStore - safe to unwrap as they're required for the app to work
        let oauth_id = secrets.get("GOOGLE_OAUTH_CLIENT_ID").unwrap();
        let oauth_secret = secrets.get("GOOGLE_OAUTH_CLIENT_SECRET").unwrap();

        let ctx = ReqwestClient::new();

        let state = AppState {
            db,
            ctx,
            key: Key::generate(),
        };

        let router = Router::new().route("/", get(hello_world));

        // More info about this below - we will build an oauth client that can interface with any OAuth service
        // Depending on the URLs we pass into it - read more here: https://docs.rs/oauth2/latest/oauth2/struct.Client.html?search=bassiclient#method.new
        let client = build_oauth_client(oauth_id, oauth_secret);

        Ok(router.into())
    }
}

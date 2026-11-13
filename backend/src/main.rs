use clap::{Parser, Subcommand};
use retro_backend::{
    VERSION, app,
    config::{Config, RunMigrations},
    connect, migrate, openapi, seed,
    state::AppState,
};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(name = "retro-backend", version = VERSION, about = "Retro API server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (default).
    Serve,
    /// Apply database migrations and exit.
    Migrate,
    /// Print the OpenAPI document and exit.
    Openapi,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Openapi) = cli.command {
        println!("{}", openapi::json());
        return Ok(());
    }

    init_tracing();
    let config = Config::from_env()?;
    let db = connect(&config.database_url).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Migrate => {
            migrate(&db).await?;
            tracing::info!("migrations applied");
            Ok(())
        }
        Command::Serve => serve(db, config).await,
        Command::Openapi => unreachable!(),
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,sea_orm=warn"));
    let builder = fmt().with_env_filter(filter);
    if std::env::var("LOG_FORMAT").as_deref() == Ok("pretty") {
        builder.init();
    } else {
        builder.json().flatten_event(true).init();
    }
}

async fn serve(db: sea_orm::DatabaseConnection, config: Config) -> anyhow::Result<()> {
    if config.run_migrations == RunMigrations::Startup {
        migrate(&db).await?;
    }
    if config.seed_dev_users {
        seed::seed_dev_users(&db).await?;
    }

    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(config.metrics_addr)
        .install()?;

    let addr = config.bind_addr;
    let router = app::router(AppState::new(db, config));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, version = VERSION, "listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { () = ctrl_c => {}, () = term => {} }
}

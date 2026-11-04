//! PostgreSQL es el único registro durable. Migraciones embebidas y conexiones asíncronas.
use anyhow::{Context, Result};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{str::FromStr, time::Duration};

pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

pub(crate) async fn connect() -> Result<PgPool> {
    let url = std::env::var("OKF_DATABASE_URL")
        .context("falta OKF_DATABASE_URL; configura la conexión en .env o con --env-file")?;
    let options = PgConnectOptions::from_str(&url)
        .map_err(|_| anyhow::anyhow!("OKF_DATABASE_URL no es una conexión PostgreSQL válida"))?
        .disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .context("no se pudo conectar a PostgreSQL")?;
    MIGRATOR
        .run(&pool)
        .await
        .context("no se pudo migrar PostgreSQL")?;
    Ok(pool)
}

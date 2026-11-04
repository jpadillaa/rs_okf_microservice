//! Cada raíz temporal usa su propio esquema en una base exclusiva de pruebas.
use sha2::{Digest, Sha256};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{path::Path, str::FromStr, time::Duration};

pub fn test_url() -> String {
    std::env::var("OKF_TEST_DATABASE_URL").ok().or_else(|| {
        dotenvy::from_path_iter(".env").ok()?.filter_map(Result::ok)
            .find(|(key, _)| key == "OKF_TEST_DATABASE_URL").map(|(_, value)| value)
    }).expect("configura OKF_TEST_DATABASE_URL en .env; nunca se usa la base del servicio para pruebas")
}

pub fn options(root: &Path) -> PgConnectOptions {
    let schema = schema(root);
    PgConnectOptions::from_str(&test_url())
        .unwrap()
        .options([("search_path", schema.as_str())])
        .disable_statement_logging()
}
fn schema(root: &Path) -> String {
    format!(
        "test_{:x}",
        Sha256::digest(root.to_string_lossy().as_bytes())
    )[..48]
        .to_owned()
}

pub async fn pool(root: &Path) -> PgPool {
    let options = options(root);
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .unwrap();
    // Identificador generado exclusivamente con un prefijo y dígitos hexadecimales.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE SCHEMA IF NOT EXISTS {}",
        schema(root)
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    pool
}

#[allow(dead_code)] // Solo las pruebas que lanzan un binario necesitan la URL.
pub fn database_url(root: &Path) -> String {
    // to_url_lossy omite las opciones de sesión: el hijo debe usar el mismo esquema.
    let mut url = options(root).to_url_lossy();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={}", schema(root)));
    url.to_string()
}

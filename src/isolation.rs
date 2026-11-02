//! Límites del hijo y acceso mínimo al sistema de archivos (plan 2, C4).
use crate::worker::Config;
use anyhow::{Context, Result};
use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
use std::{path::Path, process::ExitStatus};

fn limit(resource: Resource, soft: u64, hard: u64) -> Result<()> {
    let inherited = getrlimit(resource);
    let hard = hard.min(inherited.maximum.unwrap_or(u64::MAX));
    setrlimit(
        resource,
        Rlimit {
            current: Some(soft.min(hard)),
            maximum: Some(hard),
        },
    )
    .with_context(|| format!("no se pudo imponer {resource:?}"))
}

pub(crate) fn resource_limits(cfg: &Config) -> Result<()> {
    // RSS sigue vigilado por el padre. AS incluye reservas de bibliotecas y pilas;
    // en macOS no ofrece una garantía y no se configura.
    #[cfg(target_os = "linux")]
    {
        let address = cfg
            .max_memory_mb
            .saturating_mul(2)
            .saturating_add(128)
            .saturating_mul(1024 * 1024);
        limit(Resource::As, address, address)?;
    }
    let cpu = cfg.timeout.as_secs().max(1);
    limit(Resource::Cpu, cpu, cpu.saturating_add(1))?;
    let file = cfg.max_output_mb.saturating_mul(1024 * 1024);
    limit(Resource::Fsize, file, file)?;
    limit(Resource::Nofile, cfg.max_open_files, cfg.max_open_files)?;
    limit(Resource::Core, 0, 0)?;
    Ok(())
}

pub(crate) fn apply(cfg: &Config, original: &Path, attempt: &Path) -> Result<()> {
    resource_limits(cfg)?;
    restrict_files(original, attempt)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn landlock_abi() -> i64 {
    // Consulta sin modificar el proceso: NULL, tamaño 0, CREATE_RULESET_VERSION=1.
    unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0usize,
            1u32,
        )
    }
}

pub(crate) fn log_support() {
    #[cfg(target_os = "linux")]
    {
        let abi = landlock_abi();
        if abi < 1 {
            tracing::warn!(
                "Landlock no disponible; siguen activos rlimits, vigilancia y contenedor sin red"
            );
        } else {
            tracing::info!(abi, "Landlock disponible para el proceso conversor");
        }
    }
    #[cfg(not(target_os = "linux"))]
    tracing::info!(
        "Landlock y RLIMIT_AS no aplicables; activos los otros rlimits y la vigilancia de RSS"
    );
}

#[cfg(target_os = "linux")]
fn restrict_files(original: &Path, attempt: &Path) -> Result<()> {
    use landlock::{
        ABI, Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus, path_beneath_rules,
    };
    let abi = ABI::V3; // Incluye REFER y TRUNCATE; compatibilidad explícita con kernels anteriores.
    let read = AccessFs::ReadFile;
    let write = AccessFs::ReadFile
        | AccessFs::ReadDir
        | AccessFs::WriteFile
        | AccessFs::RemoveDir
        | AccessFs::RemoveFile
        | AccessFs::MakeDir
        | AccessFs::MakeReg
        | AccessFs::Refer
        | AccessFs::Truncate;
    let mut rules = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .create()?
        .add_rule(PathBeneath::new(PathFd::new(original)?, read))?
        .add_rule(PathBeneath::new(PathFd::new(attempt)?, write))?;
    let mut readable = vec![
        // Recursos de ejecución: fuente, reloj y máximo de memoria propio.
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into(),
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf".into(),
        "/etc/localtime".into(),
    ];
    if let Some(font) = std::env::var_os("OKF_FONT_PATH") {
        readable.push(std::path::PathBuf::from(font));
    }
    rules = rules.add_rules(path_beneath_rules(readable, read))?;
    let status = rules.restrict_self()?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        tracing::warn!(?status, "restricción Landlock incompleta");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn restrict_files(_original: &Path, _attempt: &Path) -> Result<()> {
    Ok(())
}

/// Las señales explícitas de límites no se confunden con una caída externa.
pub(crate) fn limit_failure(status: ExitStatus, stderr: &str, cfg: &Config) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(libc::SIGXCPU) => Some(format!(
            "la conversión superó el límite de CPU de {} s",
            cfg.timeout.as_secs()
        )),
        Some(libc::SIGXFSZ) => Some(format!(
            "la conversión superó el límite de archivo de {} MiB",
            cfg.max_output_mb
        )),
        Some(libc::SIGABRT)
            if stderr.contains("memory allocation of ") && stderr.contains("bytes failed") =>
        {
            Some(format!(
                "la conversión superó el límite de memoria de {} MB (asignación rechazada): {stderr}",
                cfg.max_memory_mb
            ))
        }
        _ => None,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    #[test]
    fn isolated_probe() {
        let Ok(mode) = std::env::var("RUST_TEST_ISOLATION") else {
            return;
        };
        let root = std::path::PathBuf::from(std::env::var_os("RUST_TEST_ISOLATION_DIR").unwrap());
        let cfg = Config {
            max_memory_mb: 128,
            max_output_mb: 1,
            ..Config::default()
        };
        apply(&cfg, &root.join("original"), &root.join("attempt")).unwrap();
        match mode.as_str() {
            "memory" => {
                std::hint::black_box(Vec::<u8>::with_capacity(1024 * 1024 * 1024));
            }
            "file" => {
                fs::write(root.join("attempt/large"), vec![0; 2 * 1024 * 1024]).unwrap();
            }
            "paths" => {
                assert_eq!(fs::read(root.join("original")).unwrap(), b"original");
                fs::write(root.join("attempt/allowed"), b"ok").unwrap();
                assert!(fs::write(root.join("outside"), b"no").is_err());
                assert!(fs::read(root.join("private")).is_err());
                assert!(fs::write(root.join("original"), b"no").is_err());
            }
            _ => panic!("modo desconocido"),
        }
    }

    #[test]
    fn operating_system_enforces_memory_file_and_paths() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("attempt")).unwrap();
        fs::write(tmp.path().join("original"), b"original").unwrap();
        fs::write(tmp.path().join("private"), b"private").unwrap();
        for mode in ["memory", "file", "paths"] {
            if mode == "paths" && landlock_abi() < 1 {
                assert!(
                    std::env::var_os("CI").is_none(),
                    "la CI debe comprobar Landlock activo"
                );
                eprintln!("Landlock no disponible: comprobación de rutas pendiente");
                continue;
            }
            let out = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "isolation::tests::isolated_probe", "--nocapture"])
                .env("RUST_TEST_ISOLATION", mode)
                .env("RUST_TEST_ISOLATION_DIR", tmp.path())
                .output()
                .unwrap();
            if mode == "paths" {
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
            } else {
                let message = limit_failure(
                    out.status,
                    &String::from_utf8_lossy(&out.stderr),
                    &Config::default(),
                );
                assert!(
                    message.is_some(),
                    "{mode}: {:?}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
    }
}

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

pub(crate) fn apply(cfg: &Config, original: &Path, attempt: &Path) -> Result<std::fs::File> {
    resource_limits(cfg)?;
    // Conserva el descriptor del original autorizado: no depende de reabrirlo tras
    // Landlock; esta ruta se comprueba también en los montajes de Docker Desktop.
    let input = std::fs::File::open(original).context("no se pudo abrir el original")?;
    // Los modelos de OCR se leen después de restringir el acceso, al llegar a la
    // primera página escaneada.
    let models = cfg.ocr.enabled.then_some(cfg.ocr.models_dir.as_path());
    restrict_files(original, attempt, models)?;
    restrict_network()?;
    Ok(input)
}

/// El supervisor necesita PostgreSQL, pero el conversor no puede crear sockets.
/// Se instala antes de leer el documento y antes de crear hilos; lo heredan sus hijos.
#[cfg(target_os = "linux")]
fn restrict_network() -> Result<()> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => 0xc000003e,
        "aarch64" => 0xc00000b7,
        other => anyhow::bail!("seccomp no implementado para {other}"),
    };
    let statement = |code: u32, k| libc::sock_filter {
        code: code as u16,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |code: u32, k, jt, jf| libc::sock_filter {
        code: code as u16,
        jt,
        jf,
        k,
    };
    let mut filter = vec![
        statement(libc::BPF_LD | libc::BPF_W | libc::BPF_ABS, 4), // seccomp_data.arch
        jump(libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K, arch, 1, 0),
        statement(libc::BPF_RET | libc::BPF_K, libc::SECCOMP_RET_KILL_PROCESS),
        statement(libc::BPF_LD | libc::BPF_W | libc::BPF_ABS, 0), // seccomp_data.nr
        // Rechaza la ABI x32: comparte arch con x86_64, pero cambia números de syscall.
        jump(
            libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K,
            0x40000000,
            0,
            1,
        ),
        statement(libc::BPF_RET | libc::BPF_K, libc::SECCOMP_RET_KILL_PROCESS),
    ];
    for syscall in [
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_connect,
        libc::SYS_ptrace,
        libc::SYS_io_uring_setup,
    ] {
        filter.push(jump(
            libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K,
            syscall as u32,
            0,
            1,
        ));
        filter.push(statement(
            libc::BPF_RET | libc::BPF_K,
            libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        ));
    }
    filter.push(statement(
        libc::BPF_RET | libc::BPF_K,
        libc::SECCOMP_RET_ALLOW,
    ));
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    // SAFETY: el filtro vive durante prctl, el kernel copia sus instrucciones, y
    // no_new_privs solo reduce permisos del proceso actual y sus descendientes.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0
        || unsafe { libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) } != 0
    {
        return Err(std::io::Error::last_os_error())
            .context("no se pudo aislar la red del conversor");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn restrict_network() -> Result<()> {
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
                "Landlock no disponible; siguen activos rlimits, vigilancia y seccomp sin sockets"
            );
        } else {
            tracing::info!(abi, "Landlock disponible para el proceso conversor");
        }
    }
    #[cfg(not(target_os = "linux"))]
    tracing::info!(
        "Landlock, seccomp y RLIMIT_AS no aplicables; activos los otros rlimits y la vigilancia de RSS"
    );
}

#[cfg(target_os = "linux")]
fn restrict_files(original: &Path, attempt: &Path, models: Option<&Path>) -> Result<()> {
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
    readable.extend(models.map(Path::to_path_buf));
    rules = rules.add_rules(path_beneath_rules(readable, read))?;
    let status = rules.restrict_self()?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        tracing::warn!(?status, "restricción Landlock incompleta");
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn restrict_files(_original: &Path, _attempt: &Path, _models: Option<&Path>) -> Result<()> {
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
            "network" => {
                for domain in [libc::AF_INET, libc::AF_INET6, libc::AF_UNIX] {
                    assert_eq!(unsafe { libc::socket(domain, libc::SOCK_STREAM, 0) }, -1);
                    assert_eq!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(libc::EPERM)
                    );
                }
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
        for mode in ["memory", "file", "paths", "network"] {
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
            if matches!(mode, "paths" | "network") {
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

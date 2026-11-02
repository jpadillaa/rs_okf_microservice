# Fuzzing de los lectores (plan 2, C5)

Los cuatro objetivos de `fuzz/fuzz_targets/` llaman a `fuzz_input` de la biblioteca
Rust del servicio. Reciben bytes en memoria, ejecutan el adaptador completo,
ensamblan en una carpeta temporal y validan el bundle. Reutilizan los límites
lógicos de `Config::default()` (subida, ZIP, unidades e imágenes). Los errores de
formato se descartan; los pánicos y fallos de memoria llegan a libFuzzer.

El binario conserva los mismos subcomandos; `src/main.rs` solo llama a `run()` de
`src/lib.rs`. Los límites de proceso y el gancho de pánico de C3/C4 pertenecen al
subcomando `convert`, no a los objetivos instrumentados: AddressSanitizer necesita
reservar espacio virtual y libFuzzer debe conservar el caso que provoca el fallo.
El arnés limita a 500 MiB de RSS y 300 segundos por entrada; el máximo de entrada
es 100 MiB. Cada conversión usa un directorio temporal independiente.

## Ejecutar y reproducir

```bash
rustup toolchain install nightly-2026-11-01 --profile minimal
cargo install cargo-fuzz --version 0.13.1 --locked
python3 tools/fuzz_seeds.py
cargo fetch --manifest-path fuzz/Cargo.toml --locked
CARGO_NET_OFFLINE=true cargo +nightly-2026-11-01 fuzz run docx --codegen-units 16 -- \
  -max_total_time=300 -timeout=300 -rss_limit_mb=500 -max_len=104857600
```

Se repite con `pptx`, `xlsx` y `pdf`. `fuzz/Cargo.lock` conserva las dependencias del
servicio más libFuzzer; tras cambiar dependencias del servicio se revisa también
este lockfile. `cargo-fuzz` no ofrece `--locked` en `run`: se descargan antes las
dependencias con `--locked`, se ejecuta sin red y la CI verifica que ningún lockfile
cambió.

Las semillas se copian **solo** de `examples/`; el script no lee `datos_de_prueba/`
ni el recibo privado. El corpus que libFuzzer amplía, sus artefactos y los binarios
no se versionan. Para reproducir un fallo, se pasa su archivo como corpus; para
reducirlo se usa `cargo +nightly-2026-11-01 fuzz tmin FORMATO ARCHIVO`.
Cada fallo real debe corregirse y quedar reducido como prueba de regresión antes
de considerar estable el objetivo. Los defectos en dependencias se contienen con
C3/C4 y se documentan para comunicar su caso mínimo aguas arriba.

## CI y criterio de cierre

El flujo `scheduled.yml` mantiene la auditoría diaria de dependencias y añade una
matriz de cuatro objetivos: cinco minutos de fuzzing por formato, con un tope de
45 minutos por trabajo que incluye instalación y compilación. Conserva el corpus
en caché y sube logs y casos fallidos durante 14 días, incluso cuando el objetivo
falla. Nightly se limita a este flujo; la CI normal sigue con Rust 1.99.

**Aceptación temporal pendiente:** hacen falta siete ejecuciones nocturnas
consecutivas sin fallos nuevos. La implementación y una prueba local breve no
sustituyen esa evidencia. No puede empezar hasta publicar el flujo.

La verificación de C5 detectó una carrera en la prueba del worker: una conversión de 17 ms podía terminar antes del muestreo de memoria. Se añadió la comprobación final de `getrusage` en Linux y macOS, y del tiempo total, antes de aceptar el resultado.

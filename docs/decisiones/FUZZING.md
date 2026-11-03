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
ASAN_OPTIONS='quarantine_size_mb=32:thread_local_quarantine_size_kb=256:malloc_context_size=10' \
CARGO_NET_OFFLINE=true cargo +nightly-2026-11-01 fuzz run docx --codegen-units 16 -- \
  -max_total_time=300 -timeout=300 -rss_limit_mb=500 -max_len=104857600
```

Se repite con `pptx`, `xlsx` y `pdf`. `fuzz/Cargo.lock` conserva las dependencias del
servicio más libFuzzer; tras cambiar dependencias del servicio se revisa también
este lockfile. `cargo-fuzz` no ofrece `--locked` en `run`: se descargan antes las
dependencias con `--locked`, se ejecuta sin red y la CI verifica que ningún lockfile
cambió.

Las semillas se copian **solo** de `examples/` y las regresiones sintéticas de
`fuzz/regressions/`; el script no lee `datos_de_prueba/` ni el recibo privado.
El corpus que libFuzzer amplía, sus artefactos y los binarios
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
sustituyen esa evidencia. Primero hay que resolver el hallazgo PDF descrito abajo
y publicar el flujo.

La verificación de C5 detectó una carrera en la prueba del worker: una conversión de 17 ms podía terminar antes del muestreo de memoria. Se añadió la comprobación final de `getrusage` en Linux y macOS, y del tiempo total, antes de aceptar el resultado.

La primera prueba instrumentada agotó 500 MiB con semillas intactas (`s2.pptx` y
`p2.pdf`), que en el conversor ordinario medían respectivamente 38 y 34 MiB. Se
acotó la cuarentena de AddressSanitizer a 32 MiB (256 KiB por hilo y diez marcos de
asignación), manteniendo activas sus comprobaciones y el límite RSS de 500 MiB.
Con esa configuración PPTX, XLSX y PDF completaron la comprobación breve sin nuevos
fallos. Los artefactos `oom` iniciales corresponden a ese presupuesto del arnés;
las dos semillas ya forman parte de las pruebas de regresión ordinarias.

## Primera campaña completa y hallazgo pendiente

La campaña local del 2 de noviembre, con el corpus ampliado durante las pruebas
breves y el presupuesto nocturno de 300 s por objetivo, dio estos resultados en macOS:

| Objetivo | Ejecuciones | Resultado |
| --- | ---: | --- |
| PPTX | 3269 | Sin fallos en 301 s. |
| DOCX | 41960 | Sin fallos en 301 s. |
| XLSX | 50526 | Sin fallos en 301 s. |
| PDF | — | Pánico en `pdf_oxide` 0.3.78 al leer un encabezado de objeto corrupto. |

El caso PDF procede de una mutación de la muestra sintética `breve.pdf`. Se redujo
manualmente a [566 bytes](../../fuzz/regressions/pdf/utf8-object-header.pdf), con un
encabezado de fuente que empieza por bytes no UTF-8. `has_standalone_obj_keyword`
en `src/document.rs:25363` de la dependencia usa un índice de bytes para cortar
un `str`; el índice puede caer dentro del carácter de sustitución U+FFFD.

Se reproduce con:

```bash
cargo +nightly-2026-11-01 fuzz run pdf fuzz/regressions/pdf/utf8-object-header.pdf
```

La corrección propuesta para la dependencia es comparar bytes, ya que `end` es ASCII:

```diff
-        if i >= 3 && &s[i - 3..i] == "end" {
+        if i >= 3 && &s.as_bytes()[i - 3..i] == b"end" {
```

Debe acompañarse de una prueba de `has_standalone_obj_keyword("\u{fffd}\u{fffd}0 obj")`.
La corrección **no está integrada ni enviada aguas arriba**. La prueba de proceso
`pdf_oxide_fuzz_panic_is_permanent_and_does_not_stop_the_worker` comprueba la
contención C3/C4: `failed` en el primer intento, diagnóstico conservado, ningún bundle
publicado y procesamiento posterior de un documento válido. El objetivo de fuzzing
sigue detectando el defecto; no lo captura ni lo excluye de las semillas.

**C5 no está cerrado.** Falta corregir o actualizar la dependencia, sustituir la
prueba de contención por una regresión sin pánico y obtener las siete noches.

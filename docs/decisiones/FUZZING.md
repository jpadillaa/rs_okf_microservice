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
cargo +1.99 install cargo-fuzz --version 0.13.1 --locked
python3 tools/fuzz_seeds.py
cargo fetch --manifest-path fuzz/Cargo.toml --locked
ASAN_OPTIONS='quarantine_size_mb=32:thread_local_quarantine_size_kb=256:malloc_context_size=10' \
CARGO_NET_OFFLINE=true cargo +nightly-2026-11-01 fuzz run docx --codegen-units 16 -- \
  -max_total_time=300 -timeout=300 -rss_limit_mb=500 -max_len=104857600
```

Se repite con `pptx`, `xlsx` y `pdf`; para `pdf`, la CI usa `-max_total_time=600` y
añade `-entropic_scale_per_exec_time=1` (ver [Carga del corpus PDF](#carga-del-corpus-pdf)).
`fuzz/Cargo.lock` conserva las dependencias del
servicio más libFuzzer; tras cambiar dependencias del servicio se revisa también
este lockfile. `cargo-fuzz` no ofrece `--locked` en `run`: se descargan antes las
dependencias con `--locked`, se ejecuta sin red y la CI verifica que ningún lockfile
cambió.

Las semillas se copian **solo** de `examples/` y las regresiones sintéticas de
`fuzz/regressions/`; el script no lee `datos_de_prueba/` ni el recibo privado.
Las regresiones de defectos aún abiertos (lista `OPEN` de `tools/fuzz_seeds.py`)
no se copian y se retiran del corpus restaurado: como semilla harían caer el
objetivo al cargar el corpus, antes de mutar nada.
El corpus que libFuzzer amplía, sus artefactos y los binarios
no se versionan. Para reproducir un fallo, se pasa su archivo como corpus; para
reducirlo se usa `cargo +nightly-2026-11-01 fuzz tmin FORMATO ARCHIVO`.
Cada fallo real debe corregirse y quedar reducido como prueba de regresión antes
de considerar estable el objetivo. Los defectos en dependencias se contienen con
C3/C4 y se documentan para comunicar su caso mínimo aguas arriba.

## CI y criterio de cierre

El flujo `scheduled.yml` mantiene la auditoría diaria de dependencias y añade una
matriz de cuatro objetivos: cinco minutos de fuzzing por formato (diez en PDF), con un tope de
75 minutos por trabajo que incluye instalación, compilación y el guardado de la caché.
El tope era de 45 minutos hasta la ejecución del 11 de noviembre (Programado #12): la
caché de compilación de `pdf` había sido expulsada, el trabajo compiló sin ella durante
42 minutos, fuzzeó minuto y medio sin fallos y se canceló; `docx`, también sin caché,
compiló en 35 minutos y terminó a tiempo, y los objetivos con caché tardaron unos 11.
Un trabajo cancelado no guarda su caché, así que la noche siguiente habría vuelto a
empezar en frío. Los cuatro objetivos son binarios del mismo crate y comparten una
caché de compilación (`shared-key: fuzz`) que solo escribe `main`; antes había una de
1,1 a 1,5 GB por formato. Las cachés del repositorio sumaban 10,4 GB frente al límite
de 10 GB de GitHub, que expulsa primero las menos usadas: las del fuzzing, leídas una
vez al día. Por eso las PR ya no escriben las cachés de `ci.yml` (la de Rust y la de
capas de Docker, ver [Compilación Docker](COMPILACION_DOCKER.md)). Conserva el corpus
en caché y sube logs y casos fallidos durante 14 días, incluso cuando el objetivo
falla. Nightly se limita a compilar y ejecutar los objetivos; la CI normal sigue
con Rust 1.99. `cargo-fuzz` se instala con Rust 1.99: su lockfile fija `rustix`
0.36.5, que en Linux activa `rustc_attrs` al detectar nightly y no compila con
`nightly-2026-11-01`. Ese error detuvo las ejecuciones programadas del 3 y el 4 de
noviembre antes de llegar al fuzzing, así que hasta la corrección no hubo ninguna
noche en la CI.

El paso de fuzzing se ejecuta con `shell: bash`, que activa `pipefail`. Sin él, el
`tee` que guarda el log devolvía éxito aunque cargo-fuzz fallara: en la primera
ejecución completa (5 de noviembre, lanzada a mano) el objetivo `pdf` reprodujo el
pánico conocido de `pdf_oxide` y el trabajo terminó en verde. Con `pipefail`, `pdf`
fallaba cada noche al cargar la regresión copiada como semilla, sin llegar a mutar;
desde el 6 de noviembre esa regresión no se usa como semilla mientras siga abierta,
y el objetivo vuelve a explorar (si una mutación reencuentra el mismo pánico, el log
lo identifica por su ubicación en `pdf_oxide`). Una ejecución local de 120 s en macOS,
con las opciones de ASan de la CI, cargó las 60 semillas y completó 238 ejecuciones
sin fallos; su máximo residente, 482 MB, queda cerca del límite de 500 MB del arnés. Los demás objetivos pasaron
sus cinco minutos (DOCX, 53 932 ejecuciones; XLSX, 84 048; PPTX, 3356, con dos
entradas lentas registradas por el renderizado).

### Carga del corpus PDF

Antes de mutar, libFuzzer ejecuta una vez cada entrada del corpus; la línea `INITED`
del log marca el final de esa carga, y `-max_total_time` la incluye. Con ASan, las
entradas PDF son lentas: las mutaciones de `p1.pdf` (1,5 MB) tardan de segundos a más
de un minuto. El corpus de la caché pasó en tres noches de las 8 semillas a 126
entradas, casi todas mutaciones de `p1.pdf` y `p2.pdf`, y con el presupuesto de 300 s
la carga acabó ocupándolo entero. Tiempos medidos con las marcas del log de Actions:

| Ejecución | Corpus al cargar | Carga | Mutación (ejecuciones) |
| --- | ---: | ---: | ---: |
| 7 nov., manual | 8 (3,2 MB) | 36 s | 266 s (50) |
| 7 nov. | 46 (20,8 MB) | 162 s | 142 s (99) |
| 8 nov. | 120 (41,8 MB) | 247 s | 80 s (8) |
| 9 nov. | 125 (43,6 MB) | 317 s | 0 |
| 10 nov. | 125 (43,6 MB) | 341 s | 0 |
| 11 nov., manual (`claude/fuzz-timeout-cache`) | 126 (43,7 MB) | 503 s | 0 |

El 11 de noviembre, la carga terminó a los 502 s con `#159 INITED … exec/s: 0` y el
objetivo no mutó nada. Las dos entradas lentas que escribió (12 y 56 s) no son fallos.
La ejecución programada de ese día no llegó a `INITED`: compiló en frío y el tope de
45 minutos la canceló durante la carga.

Desde el 11 de noviembre:

- `tools/fuzz_seeds.py` acota lo que se restaura de la caché. De las entradas que
  añadió libFuzzer (nombradas por el SHA-1 de su contenido) conserva las de hasta
  128 KiB, de menor a mayor, hasta sumar 4 MiB, y retira las demás. Las semillas de
  `examples/` y las regresiones se copian siempre. La carga queda cerca de la de las
  semillas solas (36 s en la CI) más la de esas entradas pequeñas, y deja de crecer con
  las noches. Las mutaciones grandes que añada una noche se pierden en la siguiente;
  los fallos se conservan como artefactos y la cobertura de `p1.pdf` y `p2.pdf` sigue
  llegando por las semillas.
- PDF fuzzea 600 s, y `-entropic_scale_per_exec_time=1` reparte la energía de
  mutación en favor de las entradas rápidas. En frío, el trabajo cabe en los 75
  minutos: unos 42 de compilación, 4 de herramientas, 10 de fuzzing y hasta 5 más si
  la última ejecución llega al `-timeout` de 300 s.

Se descartaron otras opciones. `-merge=1` y `cargo fuzz cmin` ejecutan cada entrada
como la carga: minimizar antes de cada noche costaría otros ocho minutos y el corpus
volvería a crecer. Bajar `-max_len` trunca al cargar las entradas más largas, también
las semillas `p1.pdf` y `p2.pdf`, que perderían la tabla de referencias del final.
Bajar `-timeout` convertiría en fallo cualquier entrada lenta, como las de 12 y 56 s.
Subir solo `-max_total_time` aplazaba el problema: la carga crecía cada noche.

**Criterio de cierre:** cada objetivo debe acumular dos horas de fuzzing sin fallos
nuevos, con las opciones de la CI (las de [Ejecutar y reproducir](#ejecutar-y-reproducir)).
Cuentan las ejecuciones nocturnas y las campañas locales que se registren aquí con su
fecha, duración y resultado, a partir del 6 de noviembre; no hace falta que sean
consecutivas. Solo cuenta la fase de mutación, desde `INITED` hasta `DONE`, medida con
las marcas de tiempo del log: una noche que solo cargó el corpus ejecutó entradas ya
conocidas y no exploró nada. La regla vale para los cuatro objetivos; en PPTX, DOCX y
XLSX la carga dura segundos. Con ella, PDF acumulaba al 11 de noviembre 488 s de
mutación sin fallos nuevos (las dos ejecuciones del 7 y la noche del 8); las noches del
9 y el 10, y la ejecución manual del 11, no cuentan. Un fallo nuevo
reinicia la cuenta de su objetivo cuando queda corregido y reducido como regresión. Si
`pdf` reencuentra el pánico conocido de `pdf_oxide`, identificado por su ubicación en la
dependencia, la ejecución cuenta hasta ese punto y no reinicia la cuenta. Además, C5
sigue abierto hasta resolver el hallazgo PDF descrito abajo.

Hasta el 6 de noviembre el criterio eran siete noches consecutivas sin fallos nuevos.
Se cambió porque daba poca evidencia (35 minutos por objetivo) y porque cualquier
fallo de la infraestructura volvía a empezar el calendario. Dos horas equivalen a
unas 24 noches de cinco minutos; las campañas locales permiten llegar antes.

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
La corrección **no está integrada en ninguna versión publicada**. No hace falta
reportarla: el proyecto ya la conoce.

| Referencia | Estado al 5 de noviembre de 2026 |
| --- | --- |
| [yfedoseev/pdf_oxide#2995](https://github.com/yfedoseev/pdf_oxide/issues/2995) | Mismo pánico y misma corrección, con otro disparador: un comentario con un byte no UTF-8 antes de `obj`. Cerrada como «not planned» el 31 de octubre porque, según el mantenedor, el trabajo de la 0.3.79 ya acumula la cabecera como bytes (commit `f8709f8c`, con pruebas); ese trabajo era local y no estaba en `release/v0.3.79`. Pide reabrirla si la versión publicada no lo incluye. |
| [yfedoseev/pdf_oxide#2999](https://github.com/yfedoseev/pdf_oxide/pull/2999) | Corrección mínima por comparación de bytes; abierta, sin integrar. |
| [yfedoseev/pdf_oxide#3016](https://github.com/yfedoseev/pdf_oxide/issues/3016) | Corrección estructural: leer las cabeceras de objeto como bytes en lugar de una cadena con pérdidas; abierta. |

La última versión publicada sigue siendo la 0.3.78. Cuando salga la 0.3.79 se
actualiza la dependencia y se comprueba con
`fuzz/regressions/pdf/utf8-object-header.pdf`; si el pánico persiste, se reabre
#2995 con ese caso. La prueba de proceso
`pdf_oxide_fuzz_panic_is_permanent_and_does_not_stop_the_worker` comprueba la
contención C3/C4: `failed` en el primer intento, diagnóstico conservado, ningún bundle
publicado y procesamiento posterior de un documento válido. El objetivo de fuzzing
no captura el defecto, pero excluye ese caso de las semillas hasta que la
corrección llegue a la dependencia; entonces se retira de `OPEN`.

**C5 no está cerrado.** Falta corregir o actualizar la dependencia, sustituir la
prueba de contención por una regresión sin pánico y acumular las dos horas por objetivo.

El usuario decidió conservar esta contención y dejar C5 abierto mientras se resuelve
en la dependencia, sin incorporar una copia parcheada de `pdf_oxide` al repositorio.

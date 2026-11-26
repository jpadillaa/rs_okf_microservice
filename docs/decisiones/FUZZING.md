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
python3 tools/fuzz_seeds.py docx
cargo fetch --manifest-path fuzz/Cargo.toml --locked
CARGO_NET_OFFLINE=true cargo +nightly-2026-11-01 fuzz build docx --codegen-units 16
ASAN_OPTIONS='quarantine_size_mb=32:thread_local_quarantine_size_kb=256:malloc_context_size=10' \
python3 tools/fuzz_run.py docx --minutos 120 -- -timeout=300 -rss_limit_mb=500 -max_len=104857600
```

Se repite con `pptx`, `xlsx` y `pdf`; para `pdf` se añade
`-entropic_scale_per_exec_time=1` (ver [Carga del corpus PDF](#carga-del-corpus-pdf)).
`tools/fuzz_run.py` recibe sus opciones antes de `--` y reenvía al binario las que siguen
al separador. El primer intento puede usar un presupuesto breve. El margen de tres minutos
se reserva para los reinicios tras un defecto conocido. Si el lanzador no registra ninguna
ejecución de mutación, devuelve un error, incluso cuando el binario termina con código cero.
El lanzador deja el log y el resultado en
`fuzz/` (ver [Rotación semanal por formato](#rotación-semanal-por-formato)). Para una
prueba breve sigue sirviendo `cargo +nightly-2026-11-01 fuzz run FORMATO --codegen-units 16
-- -max_total_time=300 …` con las mismas opciones.
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

Desde el 16 de noviembre, `scheduled.yml` fuzzea un formato por semana durante dos
horas ([Rotación semanal por formato](#rotación-semanal-por-formato)); la auditoría de
dependencias sigue siendo diaria. Hasta el 12 de noviembre ejecutaba cada noche una
matriz de cuatro objetivos: cinco minutos de fuzzing por formato (diez en PDF), con un tope de
75 minutos por trabajo que incluía instalación, compilación y el guardado de la caché.
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
capas de Docker, ver [Compilación Docker](COMPILACION_DOCKER.md)). Conservaba el corpus
en caché y subía logs y casos fallidos durante 14 días, incluso cuando el objetivo
fallaba. Nightly se limita a compilar y ejecutar los objetivos; la CI normal sigue
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
| 11 nov., manual, con el corpus acotado (`claude/fuzz-pdf-corpus`) | 86 (7,4 MB) | 179 s | 378 s (más de 1900), hasta el pánico conocido |
| 12 nov., manual, con el triaje (`claude/fuzz-pdf-corpus`) | 86 (7,4 MB) | 196 s | 200 s (más de 400), hasta el pánico conocido; en verde con aviso |

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

La primera ejecución con estos cambios (11 de noviembre, lanzada a mano en la rama
`claude/fuzz-pdf-corpus`) retiró 40 entradas restauradas, cargó las 86 restantes en
179 s y mutó durante 378 s, a 1-3 ejecuciones por segundo, con el corpus en aumento de
86 a 988 entradas. Se detuvo al reencontrar el pánico conocido en
`pdf_oxide-0.3.78/src/document.rs:25363` (el corte de un `str` dentro de U+FFFD), que
dejó el trabajo en rojo; el caso (1657 bytes, una mutación de `breve.pdf`) reproduce
el mismo pánico en local. No es un fallo nuevo y la noche cuenta hasta ese punto.
En macOS, con las opciones de la CI, el corpus local acotado (93 entradas,
5,7 MB) cargó en 56 s, y el completo (177 entradas, 53 MB) en 147 s, con la misma
cobertura.

### Defectos conocidos en la CI

Como PDF ya muta, es de esperar que reencuentre ese pánico muchas noches mientras la
dependencia no se corrija. Para que el rojo siga señalando solo fallos nuevos, cuando
cargo-fuzz falla, el paso de fuzzing pasa el log por `tools/fuzz_triage.py`. Si el log
termina en un pánico cuya ubicación está en `KNOWN_PANICS` para ese formato, y el único
informe de error es el `deadly signal` con que libFuzzer registra un pánico, el trabajo
termina en verde con un aviso en el resumen de la ejecución. Cualquier otro caso
conserva el código de salida de cargo-fuzz: un pánico en otra línea, en otra versión o
en el propio servicio, un segundo pánico distinto, o un informe de ASan, de memoria o
de tiempo.

La ubicación incluye la versión de la dependencia
(`pdf_oxide-0.3.78/src/document.rs:25363:`). Al actualizar `pdf_oxide` deja de
coincidir, y si el defecto persiste la noche vuelve a fallar. El caso se sube como
artefacto igual que antes. Como el trabajo termina en verde, la caché también guarda el
corpus de la noche, que antes se perdía con el fallo. Para la cuenta de C5 nada
cambia: la noche cuenta hasta el pánico, y el aviso lo identifica. La ejecución del 12
de noviembre (Programado #15) lo comprobó: reencontró el pánico tras 200 s de mutación,
terminó en verde con el aviso, subió el caso y guardó el corpus en la caché.

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
XLSX la carga dura segundos. Con ella, PDF acumulaba al 12 de noviembre 1066 s de
mutación sin fallos nuevos: las dos ejecuciones del 7, la noche del 8 y las dos
ejecuciones manuales con el corpus acotado (11 y 12), cada una hasta el pánico conocido. Las noches del 9 y el 10,
y la ejecución manual anterior del 11, no cuentan. Un fallo nuevo
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

## Rotación semanal por formato

El 12 de noviembre, la ejecución programada (Programado #16) no llegó a empezar: GitHub
rechazó sus trabajos porque la cuenta había agotado los minutos de Actions o su límite
de gasto. El repositorio es privado y los minutos se cobran. Una noche con caché costaba
unos 48 minutos (cuatro trabajos de 10 a 16), cerca de 1450 al mes, y una noche en frío
unos 180. Además, cinco minutos repartidos entre la carga y la mutación daban poca
evidencia: PPTX tenía el mismo problema que PDF, con 445 entradas (44 MB) de hasta 73 s
cada una por el renderizado, y desde el 6 de noviembre solo había mutado 69 s.

Desde el 16 de noviembre, el fuzzing se ejecuta **una vez por semana, un formato cada
vez**, en este orden: XLSX, PPTX, DOCX y PDF. El paso «Formato de la semana» cuenta las
semanas desde el lunes 16 de noviembre de 2026 (XLSX) y repite la rotación cada cuatro
semanas. Cada ejecución fuzzea **120 minutos**, lo que el criterio pide por formato, de
modo que una semana puede completar las dos horas de su formato. Una semana sin uso
basta para que GitHub expulse la caché de compilación, así que el trabajo cuenta con
compilar en frío: unos 42 minutos más 4 de herramientas, los 120 de fuzzing y hasta 5
si la última entrada llega al `-timeout`, con un tope de 190. Son unos 175 minutos por
semana, unos 750 al mes. A mano (`workflow_dispatch`) se puede elegir el formato y los
minutos.

- **Corpus.** Cada formato vuelve cuatro semanas después, cuando la caché de Actions ya
  lo habría expulsado. El corpus se guarda como artefacto `fuzz-corpus-FORMATO` durante
  35 días, y cada ejecución descarga el último que dejó `main`. `tools/fuzz_seeds.py` lo
  acota al restaurarlo y antes de guardarlo: el límite de PDF se aplica también a PPTX, y
  XLSX y DOCX conservan las entradas de hasta 256 KiB hasta sumar 16 MiB. En una prueba
  local de cuatro minutos, XLSX pasó de 3 entradas a 1439 (22 MB); minimizarlas con
  `-merge=1` solo las redujo a 998, así que el límite por tamaño es el que acota el
  almacenamiento de artefactos.
- **Reinicios.** `tools/fuzz_run.py` ejecuta el binario compilado en lugar de
  `cargo fuzz run`, que tras un fallo vuelve a ejecutar un caso para mostrarlo (el 12 de
  noviembre eligió una entrada lenta y añadió cuatro minutos y medio). Si un intento
  termina en un defecto conocido después de `INITED`, conserva el caso, acota el corpus
  y vuelve a empezar con el tiempo que quede, hasta 30 veces. Un defecto conocido antes
  de mutar, o cualquier otro fallo, deja el trabajo en rojo.
- **Resultados.** Cada ejecución deja `fuzz/FORMATO-resultado.json` (formato, fecha,
  ejecución, commit, carga, mutación, ejecuciones, defectos conocidos y resultado) en el
  artefacto `fuzz-resultado`, durante 90 días, y una fila en el resumen de la ejecución.
  `tools/fuzz_report.py` los suma por formato a lo acumulado antes de la rotación y
  muestra lo que falta para las dos horas. Como los artefactos caducan, el registro que
  perdura es la tabla de abajo, que se actualiza con esa salida.

**XLSX solo con contenido estático.** El servicio se prueba con libros de contenido
estático: texto, tablas de valores fijos y gráficos con sus datos ya calculados. Un libro
con macros o fórmulas no tiene sentido como fuente para el RAG. `tools/fuzz_seeds.py`
genera las semillas XLSX a partir de las muestras de `examples/xlsx/`: quita las
fórmulas de las hojas y conserva el valor guardado de cada celda, retira `calcChain.xml`
y la orden de recalcular al abrir, y descarta un libro con macros (`vbaProject.bin` u
hojas de macros de Excel 4). Las mutaciones siguen cambiando bytes al azar; lo que
cambia es la zona que explora el fuzzer. Las noches anteriores usaban semillas con
fórmulas y no cuentan para XLSX: sumaban 1821 s de mutación sin fallos.

### Registro de C5

La tabla recoge las ejecuciones de GitHub Actions desde el 6 de noviembre. El objetivo
por formato es de 7200 s de mutación. Se actualiza con `python3 tools/fuzz_report.py`.

| Formato | Acumulado | Origen | Estado |
| --- | ---: | --- | --- |
| XLSX | 0 s | Empieza con las semillas estáticas el 16 de noviembre. | Pendiente |
| PPTX | 69 s | Noche del 7 de noviembre; las demás solo cargaron el corpus. | Pendiente |
| DOCX | 1796 s | Ocho ejecuciones, del 7 al 12 de noviembre. | Pendiente |
| PDF | 1066 s | Cuatro ejecuciones, cada una hasta el pánico conocido. | Pendiente; además espera la corrección de `pdf_oxide` |

**Verificación local de XLSX del 18 de noviembre de 2026.** Tras corregir la lectura de
argumentos del lanzador, una campaña en Linux ARM64 con las tres semillas estáticas públicas
completó 5 095 302 ejecuciones y 7201 s de mutación. El proceso terminó con código 0 y
resultado `sin fallos`. Se usaron el nightly y los límites del flujo semanal, con LLD para
el enlace en ARM64. La campaña se ejecutó sin red y solo se conservaron sus contadores.
Esta medición local cumple el objetivo de dos horas de XLSX y se registra por separado
del acumulado de GitHub Actions.

**Verificación local de PPTX, DOCX y XLSX del 25 y 26 de noviembre de 2026.** Se repitió
la campaña local tras mover las referencias de bloque a ficheros `.spans.json` (perfil
1.24.0), sobre el commit `a265bda`, en Docker con Linux ARM64, `nightly-2026-11-01`, LLD
y las opciones del flujo semanal (`-timeout=300 -rss_limit_mb=500 -max_len=104857600`).
Ninguna campaña encontró fallos:

| Formato | Ejecuciones | Mutación medida por el lanzador | Reloj de libFuzzer |
| --- | ---: | ---: | ---: |
| DOCX | 1 080 562 | 6241 s (carga de 78 s) | 7201 s |
| XLSX | 1 872 286 | 6251 s (carga de 68 s) | 7201 s |
| PPTX | 255 662 | 7143 s (carga de 59 s) | 7201 s |

- **PPTX** cumple el objetivo de dos horas con el criterio del lanzador (presupuesto de
  7200 s menos la carga) y sin ninguna suspensión del equipo.
- **DOCX y XLSX** son la segunda tanda de ese día, tras una primera invalidada porque el
  Mac se durmió y la máquina virtual de Docker se suspendió. En la segunda, el lanzador
  mide con un reloj monotónico que excluye el tiempo suspendido, y libFuzzer con el reloj
  de pared; la diferencia de unos 880 s indica que el equipo volvió a suspenderse una
  parte de la ejecución. Con la medición del lanzador, DOCX queda unos 960 s por debajo
  de las dos horas y no cierra su objetivo; XLSX ya lo cerró el 18 de noviembre.
- **PPTX necesitó `-rss_limit_mb=1000`.** Con 500 MB una entrada del corpus agotó la
  memoria del arnés (421 MB con el código nuevo y 438 MB con el anterior para la misma
  entrada), así que no depende de este cambio. La rotación semanal de PPTX en la CI usa
  500 MB y puede toparse con el mismo límite; el flujo no se ha modificado.
- Dos intentos de PPTX se descartaron antes de esta campaña: uno por la suspensión del
  equipo, y otro porque el corpus había crecido a 1734 entradas (194 MB) y su carga
  provocó un `timeout` espurio. La campaña válida partió de las siete semillas estáticas
  y con el equipo conectado a la corriente, y no sufrió suspensiones.
- Para repetir una campaña larga en macOS hay que conectar el equipo a la corriente:
  `caffeinate` no impide la suspensión con batería.

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
corrección llegue a la dependencia; entonces se retira de `OPEN` y de `KNOWN_PANICS`
(`tools/fuzz_triage.py`).

**C5 no está cerrado.** Falta corregir o actualizar la dependencia, sustituir la
prueba de contención por una regresión sin pánico y acumular las dos horas por objetivo.

El usuario decidió conservar esta contención y dejar C5 abierto mientras se resuelve
en la dependencia, sin incorporar una copia parcheada de `pdf_oxide` al repositorio.

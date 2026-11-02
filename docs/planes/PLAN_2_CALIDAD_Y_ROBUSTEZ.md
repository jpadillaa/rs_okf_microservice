# Plan 2: calidad y robustez del microservicio

**Fecha:** 1 de noviembre de 2026.  
**Punto de partida:** el [estado del proyecto](../estado/ESTADO_DEL_PROYECTO.md) (límites conocidos, §3.3, y brechas, §4).  
**Plan complementario:** [Plan 1: recuperar lo que vale la pena del plan original](PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md). Los identificadores `R1`…`R9` remiten a ese plan.

## 1. Objetivo

Añadir las garantías que el plan inicial no preveía y que un servicio que procesa documentos no confiables necesita para operar sin supervisión constante:

- **regresión:** detectar en la CI cualquier cambio en el contenido de un bundle;
- **entradas hostiles:** que un archivo malicioso o defectuoso no tumbe el servicio, no se reintente en vano y no escape de sus límites;
- **operación:** apagado ordenado, salud, métricas y retención (el rechazo de trabajos por falta de espacio está en `R4`);
- **contrato:** saber con qué versión del conversor y del perfil se generó cada bundle, y poder validarlo después.

Tamaños relativos: **S** (un cambio acotado en un módulo), **M** (varios módulos o una herramienta nueva), **L** (cambio transversal o infraestructura nueva).

## 2. Regresión

### C1. Bundles de referencia (M)

**Problema.** Las pruebas comprueban hechos concretos (títulos, número de conceptos, estado). Un cambio que estropea un párrafo que ninguna prueba mira pasa en verde.

**Trabajo.**

1. Modo determinista de conversión para las pruebas: fecha de `generated.at` fija y duración omitida del informe (hoy ambas cambian en cada ejecución).
2. Guardar, para cada muestra y cada documento del corpus de los cuatro formatos, el bundle esperado como árbol de archivos en `examples/<formato>/esperado/<documento>/`: los Markdown completos y, en lugar de los binarios, una lista de recursos con su SHA-256.
3. Una prueba que convierte cada documento y compara archivo por archivo, mostrando la diferencia legible del primer archivo distinto.
4. Una forma explícita de regenerar las referencias tras un cambio intencionado (por ejemplo, `OKF_UPDATE_GOLDEN=1 cargo test`), cuya diferencia se revisa en el commit.
5. Prueba de determinismo: convertir dos veces el mismo original da bundles idénticos.

**Aceptación.** Los 26 documentos de ejemplo actuales (fixtures, muestras y corpus reducido) tienen referencia; un cambio deliberado en una regla de Markdown hace fallar la prueba con una diferencia legible.

### C2. Cobertura como control de la CI (S)

**Estado:** implementado el 2 de noviembre: `src/coverage.rs`, 26 mínimos en
`examples/coverage-thresholds.json`. Se conservan las listas completas y las cifras
medidas menos 0,50 puntos, según la concreción solicitada; véase [el método](../decisiones/COBERTURA_INDEPENDIENTE.md#6-control-de-regresión-en-rust-plan-2-c2).

**Trabajo.** Una prueba en Rust que calcula la cobertura de palabras de cada bundle del corpus frente a las listas de referencia de `R2` y falla por debajo de un umbral por documento (el valor medido menos un margen pequeño, por ejemplo 0,5 puntos). Las diferencias explicadas en `R2` (encabezados registrados una vez, texto de diseño) se descuentan en la lista de referencia, no en el umbral.

**Aceptación.** Un cambio que pierde texto de un documento del corpus hace fallar la CI aunque su bundle de referencia se regenere.

**Depende de:** `R2`.

## 3. Entradas hostiles

### C3. Un pánico es un fallo permanente (S)

**Estado:** implementado el 2 de noviembre; código 5 y prueba real de proceso que conserva mensaje y ubicación, falla en el primer intento y mantiene reintentables los códigos desconocidos.

**Problema.** En `run_child` (`src/worker.rs`), un proceso hijo que termina por pánico, señal o código desconocido se clasifica como fallo transitorio. Un archivo que provoca un pánico por un defecto del conversor se reintenta `OKF_MAX_ATTEMPTS` veces, con espera creciente, y falla igual.

**Trabajo.**

1. El subcomando `convert` instala un gancho de pánico que escribe el mensaje y la ubicación en la salida de error y termina con un código propio.
2. El worker clasifica ese código como permanente, con un error que distingue «defecto del conversor» de «archivo inválido» y conserva el mensaje del pánico.
3. Una terminación por señal sigue siendo transitoria, salvo que el worker la haya provocado por un límite (ya permanente).

**Aceptación.** Una prueba que provoca un pánico en el proceso hijo termina en `failed` en el primer intento, con el mensaje en el error.

### C4. Límites duros del sistema operativo para el proceso hijo (M)

**Estado:** implementación del 2 de noviembre en `src/isolation.rs`: rlimits antes de crear hilos, Landlock en Linux y prueba de rechazo de memoria, archivo y rutas.

**Problema.** La memoria se vigila consultándola cada 100 ms; un pico rápido puede superar el límite antes de que el worker lo vea. El proceso hijo puede además abrir red, escribir donde quiera y crear archivos de cualquier tamaño.

**Trabajo.**

1. Antes de convertir, el proceso hijo se impone límites con `setrlimit`: espacio de direcciones (memoria máxima con margen), CPU (el plazo de conversión), tamaño de archivo escrito y archivos abiertos. Requiere una dependencia pequeña (`libc` o `rustix`).
2. La vigilancia por consulta se mantiene como segunda capa: en macOS el límite de espacio de direcciones no se aplica.
3. En Linux, restringir el sistema de archivos a la lectura del original y la escritura en la carpeta del intento (Landlock) y ejecutar el contenedor del worker sin red; si Landlock no está disponible, el worker lo registra al arrancar.
4. Documentar las capas de aislamiento en la arquitectura técnica, §5.

**Aceptación.** Una prueba que reserva memoria por encima del límite termina de inmediato, clasificada como límite de memoria; en Linux, un intento de escribir fuera de la carpeta del intento falla.

### C5. Fuzzing de los lectores (L)

**Trabajo.**

1. Objetivos de `cargo-fuzz` para cada formato que llaman a la conversión en memoria, con los límites del servicio, partiendo de las muestras y del corpus como semillas.
2. Un flujo de CI programado (nocturno, con duración acotada por objetivo) que ejecuta el fuzzing y guarda los casos que fallan. El mismo flujo comprueba los avisos de seguridad de las dependencias (`R9`).
3. Cada caso encontrado se corrige y entra como prueba de regresión, reducido al mínimo.

**Aceptación.** Cada objetivo corre su presupuesto nocturno sin fallos nuevos durante una semana; los fallos encontrados tienen prueba y corrección.

**Riesgo:** necesita el toolchain nightly solo para el fuzzing; el resto de la CI sigue en la versión fijada. Los fallos dentro de `pdf_oxide` o `betteroffice-pptx` se reportan aguas arriba y, mientras tanto, se contienen con `C3` y `C4`.

## 4. Operación

### C6. Apagado ordenado (S)

**Trabajo.** Al recibir SIGTERM o SIGINT, el worker deja de reclamar trabajos, espera al proceso hijo hasta `OKF_SHUTDOWN_GRACE_S` y, si no termina, lo detiene y devuelve el trabajo a `queued` sin consumir un intento. La API deja de aceptar conexiones nuevas y termina las peticiones en curso.

**Aceptación.** Una prueba envía la señal durante una conversión y comprueba que el trabajo vuelve a la cola con su intento intacto, o termina publicado si acabó dentro del plazo.

### C7. Salud y métricas (M)

**Trabajo.**

1. `GET /health`: base de datos accesible, espacio libre por encima de `OKF_MIN_FREE_MB` (definida en `R4`) y último latido del worker reciente (el worker escribe su latido en SQLite).
2. `GET /metrics` en formato de texto Prometheus, escrito sin dependencias nuevas: trabajos por estado, conversiones por formato y resultado, duración por formato, memoria máxima y reintentos. Las duraciones salen de los datos por etapa que registra `R5`.

**Aceptación.** Pruebas de la API para ambos endpoints; `/health` falla si el worker deja de latir.

### C8. Retención (S)

**Trabajo.**

1. `OKF_RETENTION_DAYS`: el worker borra periódicamente los originales y bundles de trabajos terminados más antiguos que ese plazo y marca el trabajo como expirado; la descarga responde `410 Gone`.
2. Definir la política en la arquitectura técnica (§2 la exige).

El rechazo de trabajos nuevos cuando falta espacio (`507 Insufficient Storage`) es control de admisión y está en `R4`.

**Aceptación.** Pruebas de expiración; sin la variable, nada se borra (comportamiento actual).

**Decisión abierta:** plazo de retención por defecto.

## 5. Contrato

### C9. Versión del conversor y del perfil (S)

**Problema.** `generated.by` es siempre `process:document-conversion`; no se puede saber qué bundles regenerar cuando cambia una regla.

**Trabajo.**

1. Dar versión al perfil documental (una línea en la [especificación](../especificacion/ESPECIFICACION_OKF.md) y un registro de cambios al final).
2. Registrar la versión del conversor (versión del paquete y commit) y la del perfil en `documento.md`, en el informe y en el registro de trabajos.
3. Regla de trabajo: todo cambio de una regla de la especificación sube la versión del perfil en el mismo commit.

**Aceptación.** Los bundles de referencia (`C1`) muestran ambas versiones; la consulta de estado de un trabajo las devuelve.

### C10. Validador independiente (S)

**Trabajo.** Un subcomando `okf-microservice validate <bundle.zip | directorio>` que ejecuta la conformidad OKF y la integridad editorial, usando el original incluido en `references/`, y termina con un código distinto de cero si el bundle no es válido.

**Aceptación.** Valida todos los bundles de referencia de `C1` y rechaza las alteraciones que ya prueban las pruebas del validador.

## 6. Orden

| Fase | Trabajos | Motivo |
| --- | --- | --- |
| A. Base de regresión | `C1`, `C2` (con `R2` del plan 1) | Sin ella, ningún otro cambio puede comprobarse; protege la migración `R1`. |
| B. Contención | `C3`, `C4` | Cambios pequeños con mucho efecto en seguridad; preparan el fuzzing. |
| C. Contrato | `C9`, `C10` | Conviene tenerlos antes de que `R1` cambie todos los bundles. |
| D. Búsqueda de fallos | `C5` | Necesita `C3` y `C4` para que los fallos se contengan y se clasifiquen bien. |
| E. Operación | `C6`, `C7`, `C8` | Se cierran junto con las imágenes de `R3`, que es cuando el servicio se despliega. |

**Fuera de este plan:** despliegue multinodo, autenticación (se delega en un proxy delante de la API) y OCR, que sigue en el roadmap del [estado del proyecto](../estado/ESTADO_DEL_PROYECTO.md#6-roadmap-pendiente).

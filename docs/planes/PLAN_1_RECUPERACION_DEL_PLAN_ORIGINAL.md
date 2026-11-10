# Plan 1: recuperar lo que vale la pena del plan original

**Fecha:** 1 de noviembre de 2026.  
**Punto de partida:** la comparación con el plan inicial del [estado del proyecto, §5](../estado/ESTADO_DEL_PROYECTO.md#5-estado-frente-al-plan-inicial).  
**Plan complementario:** [Plan 2: calidad y robustez](PLAN_2_CALIDAD_Y_ROBUSTEZ.md). Los identificadores `C1`…`C10` remiten a ese plan.

## 1. Objetivo

Recuperar del plan inicial (commit `802b9b1`) las decisiones que siguen aportando calidad o capacidad de despliegue, y cerrar formalmente las que se abandonan, de modo que la documentación y el código vuelvan a decir lo mismo.

El plan termina cuando:

- la especificación, las arquitecturas y el stack describen lo que hace el código, y cada decisión inicial abandonada está cerrada en su documento (§3);
- un único ensamblador escribe los bundles de los cuatro formatos;
- cada formato con corpus real tiene una referencia de cobertura independiente y reproducible;
- el servicio se despliega desde una imagen reproducible, con límites configurables y control de admisión;
- el registro permite seguir cada trabajo por formato, etapa y duración;
- la CI comprueba las licencias y los orígenes de las dependencias.

El corpus (`R6`) no forma parte de esta condición: avanza según lleguen documentos. Al cerrar el plan, lo que falte del corpus queda como pendiente en el [estado del proyecto](../estado/ESTADO_DEL_PROYECTO.md).

## 2. Trabajos

Tamaños relativos: **S** (un cambio acotado en un módulo o en la documentación), **M** (varios módulos o una herramienta nueva), **L** (cambio transversal a los cuatro adaptadores).

### R1. Modelo de salida común y ensamblador único (L)

**Problema.** Cada adaptador construye su propio bundle: frontmatter escrito con `format!`, marcadores, índices, `documento.md` e informe. Una regla común se implementa cuatro veces y ya diverge: DOCX y PDF tienen copias casi idénticas de `Heading`, `Section` y `TitleSource` (`src/docx/mod.rs` y `src/pdf/mod.rs`), y cada adaptador limpia de forma distinta los caracteres de control antes de escribirlos.

**Qué se recupera y qué no.** El plan inicial preveía un modelo intermedio común *antes* del ensamblado, del que saldrían también los `source_spans`. Este trabajo no lo recupera: cada adaptador conserva su modelo de lectura y solo se unifica lo que entrega. La renuncia se registra en §3.

**Trabajo.**

1. **Secciones compartidas (M).** Extraer a un módulo común el esquema de secciones de DOCX y PDF (encabezados, secciones, origen del título y elección del nivel de división), sin cambiar sus bundles.
2. **Tipos de salida.** Definir en `src/bundle.rs` (o un módulo nuevo) `Concept` (ruta, `type`, título, descripción, `status`, campos propios del formato, `source_spans`, bloques con su marcador), `Asset`, `Issue` y `Bundle` (documento, conceptos, recursos, informe).
3. **Ensamblador único.** Escribir el frontmatter serializando con `yaml_serde`, no con plantillas de texto; generar índices, marcadores y el esqueleto del informe en un solo sitio; limpiar los caracteres de control con una sola regla.
4. **Migración.** Migrar los adaptadores uno por uno: primero PDF y DOCX, que ya comparten las secciones del paso 1; después XLSX y PPTX. Cada adaptador conserva su modelo de entrada y solo cambia lo que entrega.
5. **Etapas.** Separar en la entrada de cada adaptador la lectura del ensamblado (en PPTX, el renderizado de vistas queda en el ensamblado), para que `R5` mida las etapas en un solo sitio.
6. **Documentación.** Al terminar, completar en la [arquitectura conceptual, §3](../arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md#3-modelos-de-lectura-y-modelo-de-salida) y en el [stack, §3](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md#3-extracción-y-procedencia-por-formato) la descripción del modelo de salida común que `R7` deja anunciada.

**Aceptación.** Tras migrar cada formato, sus bundles de referencia (`C1`) no cambian, salvo diferencias revisadas y justificadas. Ningún adaptador escribe frontmatter con plantillas de texto ni define sus propios tipos de sección.

**Depende de:** `C1` (sin bundles de referencia la migración no puede comprobarse).  
**Riesgo:** diferencias de formato YAML (comillas, orden de claves) que cambian todos los bundles; se aceptan una vez, revisadas, en un commit aparte.

### R2. Referencia de cobertura independiente (M)

**Problema.** El plan inicial exigía verificar la cobertura contra el original. En PDF se midió con pdfium, pero el script no está en el repositorio y la medición no puede repetirse; en PPTX, DOCX y XLSX no hay ninguna referencia independiente.

**Trabajo.**

1. Versionar el script de la medición PDF con pdfium, fuera del servicio (Python, en `examples/` o `tools/`), y repetir con él las cifras del estado del proyecto.
2. Extender el script a OOXML con herramientas independientes del lector propio: `python-docx` para DOCX, `python-pptx` para PPTX y `openpyxl` para los valores de XLSX. Donde esas herramientas no llegan (cuadros de texto, notas al pie y comentarios de DOCX; SmartArt de PPTX), el script lee directamente la parte XML correspondiente o declara la parte como no cubierta por la referencia.
3. Medir en los dos sentidos y explicar cada diferencia: palabras de la referencia que faltan en el bundle (pérdidas) y palabras del bundle que no están en la referencia (texto que la referencia no ve, o texto inventado o duplicado). Las diferencias esperadas (encabezados registrados una vez, texto de diseño, uniones de palabras) se documentan por documento.
4. Guardar las listas de palabras de referencia como datos del repositorio, para que `C2` las use en la CI sin Python. **Solo de documentos versionados**: los que se conservan únicamente en local por privacidad (como el recibo PDF) se miden en local y sus listas no se versionan.
5. La cobertura de las muestras sintéticas no cuenta como evidencia independiente: se generan con las mismas bibliotecas que sirven de referencia y solo contienen lo que esas bibliotecas saben escribir. Se mide para `C2`, pero la evidencia de calidad sale del corpus real.

**Aceptación.** Cobertura medida en los dos sentidos y explicada para cada documento real de PDF, DOCX y PPTX; la medición PDF se reproduce con el script versionado. XLSX se mide en cuanto `R6` aporte un libro real.

### R3. Imagen reproducible (M)

**Trabajo.**

1. `Dockerfile` de varias etapas: compilación con `cargo build --release --locked` sobre una imagen base fijada por digest y una imagen de ejecución mínima con la fuente TrueType del renderizado PPTX (`OKF_FONT_PATH`).
2. Una sola imagen con dos comandos (`api` y `worker`): el ejecutable es único y la diferencia de tamaño es la fuente. El plan inicial preveía dos imágenes; la desviación se registra en §3.
3. Un `compose.yaml` con los dos procesos y el volumen persistente compartido (`OKF_DATA_DIR`).
4. Un trabajo de CI que construye la imagen y hace una prueba de humo: enviar una muestra de cada formato a la API, ejecutar `worker --once` y descargar los bundles.
5. Fijar también la fuente que instala la CI de pruebas (hoy `fonts-dejavu-core` de `apt-get`, sin versión), para que pruebas e imagen rendericen con la misma.
6. Registrar en el stack las versiones de la imagen base, de Rust y de la fuente.

**Aceptación.** La imagen se construye solo con versiones fijadas (imagen base por digest, toolchain de `rust-version`, `Cargo.lock`, paquete de la fuente), la CI de pruebas usa la misma fuente y la prueba de humo pasa en la CI.

### R4. Límites configurables y control de admisión (M)

**Trabajo.**

1. `OKF_MAX_UPLOAD_MB` en lugar de la constante `MAX_UPLOAD` (100 MB).
2. Límites de expansión ZIP configurables (`MAX_ZIP_ENTRY`, `MAX_ZIP_TOTAL`, `MAX_ZIP_ENTRIES` de `src/ooxml.rs`, usados también en `src/xlsx/read.rs`). Hoy son constantes: hay que llevar la configuración hasta los lectores OOXML, igual que los demás límites que el worker pasa al proceso hijo.
3. Límite de tamaño de las imágenes que se decodifican o renderizan (vistas PPTX y PDF), que la regla 3 del stack inicial también pedía: una imagen que lo supere no se decodifica y se declara como pérdida en el informe.
4. `OKF_MAX_QUEUED`: la API responde `429 Too Many Requests` cuando la cola supera ese número de trabajos pendientes.
5. `OKF_MIN_FREE_MB`: la API responde `507 Insufficient Storage` si el espacio libre del volumen de datos baja de ese valor. `C7` usa la misma variable en `/health`.
6. Añadir las variables a la tabla de la [arquitectura técnica, §5](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf).

**Aceptación.** Pruebas de la API para subida demasiado grande, cola llena y disco sin espacio; prueba de una imagen por encima del límite; los valores por defecto no cambian el comportamiento actual (sin límite de cola ni de espacio libre).

### R5. Registro por formato, etapa y duración (S)

**Trabajo.**

1. El proceso de conversión mide la duración de cada etapa (lectura, ensamblado, validación) y la devuelve al worker junto con el resultado, en una línea estructurada que sustituye a la actual (`estado<TAB>ruta`).
2. El worker registra al terminar cada trabajo una línea con `job_id`, intento, formato, estado, duración por etapa, duración total y memoria máxima.
3. Las métricas de duración de `C7` se calculan a partir de estos mismos datos, no de una medición aparte.

**Aceptación.** Una conversión completa produce esa línea, comprobada en la prueba del proceso hijo; ningún registro incluye contenido del documento.

**Depende de:** `R1`, paso 5 (las etapas se separan una sola vez en lugar de instrumentar cada adaptador).

### R6. Corpus representativo (M, continuo)

**Trabajo.** Incorporar, reducidos como el corpus actual y sin datos personales:

- un PDF escaneado real, un PDF mixto (páginas digitales y escaneadas) y un PDF largo de texto (libro o tesis con licencia que permita versionarlo);
- libros XLSX reales con varias hojas, regiones desplazadas, gráficos y fórmulas;
- DOCX guardados por Word, que retoman la evaluación DOCX en pausa, y al menos uno real con notas y revisiones pendientes;
- presentaciones con tablas nativas, gráficos o SmartArt, para revisar el renderizado PPTX.

Fuentes preferidas cuando el equipo no aporte archivos: portales de datos abiertos que publiquen XLSX, repositorios institucionales de tesis y libros escaneados de dominio público, siempre con la licencia registrada junto al documento.

Cada documento nuevo entra con su bundle de referencia (`C1`), su medición de cobertura (`R2`) y su umbral (`C2`), y con duración, CPU y memoria máxima medidas, como pedía la regla 5 del stack inicial (hoy la CPU solo se mide en PPTX).

**Aceptación.** El corpus cubre todos los casos de la regla 5 del stack inicial con al menos un documento real.

**Avance (7 de noviembre).** Llegó un primer PDF escaneado real: una página limpia, generada como imagen. Se midió en local con el OCR ([OCR local](../decisiones/OCR_LOCAL.md), §4) y se queda en `datos_de_prueba/`, sin incorporar al repositorio. Siguen pendientes un escaneo de escáner o de fotografía, de varias páginas, y el resto de documentos de la lista.

**Avance (9 de noviembre).** Se evaluó un PDF privado de 346 páginas y 30,02 MiB, con texto digital, tablas, figuras y código. La [prueba real](../decisiones/PRUEBAS_PDF_REALES.md) registra cobertura, validación, latencia, CPU/RSS y carga con uno y dos workers. El perfil 1.11.0 corrige enlaces que impedían convertirlo y reconoce sus títulos fragmentados: 21 conceptos y 179 de 214 marcadores; conserva el 99,54 % del texto ajustado frente a PDFium. El original no se incorpora al corpus público ni se crea su lista de palabras. R6 continúa. La segunda evaluación del perfil 1.12.0 corrige las tablas y el código seleccionados: verifica 42 celdas frente a rectángulos PDFium, 28 líneas y tres líneas vacías con su sangría, y dos listados mediante análisis sintáctico de Python sin ejecutarlos. Añade regresiones sintéticas y públicas, y vuelve a medir el ZIP servido por 8080; véanse las cifras finales y límites en la decisión de pruebas reales. Falta una regresión reducida autorizada que conserve esta combinación de maquetación, ampliar los casos y completar el resto del corpus.

**Avance (10 de noviembre).** El perfil 1.13.0 aborda las cabeceras variables y la numeración lateral: exige alineación con folios confirmados y recurrencia de posición, conserva las cabeceras con sus páginas en el inventario y reúne títulos decimales o de apéndice solo por coincidencia completa con sus marcadores. La nueva auditoría usa 303 cabeceras y 152 títulos del cuerpo extraídos con PDFium, repite los controles de tablas y código y compara el ZIP servido por 8080 con el anterior. Las cifras finales, los recursos del intento y los límites figuran en [pruebas reales](../decisiones/PRUEBAS_PDF_REALES.md). R6 sigue en curso para el resto del corpus.

**Segundo original del 10 de noviembre.** Se evaluó un cuaderno privado apaisado de 65 páginas con tablas, diagramas y 116 campos de formulario vacíos. La conversión de 8080 con 1.13.0 es `complete` y válida en 0,599 s y 70 MiB; cinco conversiones adicionales son equivalentes y no presentan fallos de salud. La cobertura ajustada global del 99,45 % oculta problemas estructurales: columnas y escalas mezcladas, navegación girada fragmentada, marcadores sin aprovechar, 63 referencias internas sin resolver y diagramas vectoriales sin representación. La [prueba real](../decisiones/PRUEBAS_PDF_REALES.md#segundo-pdf-real-cuaderno-apaisado-con-formularios-perfil-1130) registra controles independientes y prioridades de corrección. Esta evaluación no cambia el perfil ni incorpora contenido privado al corpus público; R6 continúa abierto.

**Depende de:** archivos que aporte el equipo o fuentes públicas aprobadas; revisión de licencia y privacidad de cada uno.

**Corrección y regresión (10 de noviembre, perfil 1.14.0).** Se reconoce la exportación por diapositivas y se conserva un concepto por página, con títulos reunidos por geometría y coincidencia completa con el marcador, navegación lateral inventariada con destinos resueltos por identidad de página, tablas con reglas y columnas de controles, y vistas de los diagramas. Las dos conversiones finales desde 8080 pasan el validador: el cuaderno supera los controles estructurales seleccionados y el PDF largo conserva exactamente su contenido anterior, salvo metadatos de versión y ejecución. Las 27 muestras públicas mantienen contenido y cobertura; se añaden regresiones sintéticas sin texto privado. La [reevaluación](../decisiones/PRUEBAS_PDF_REALES.md#corrección-del-cuaderno-y-regresión-de-ambos-pdf-perfil-1140) registra recursos, coste de las vistas y límites de cobertura. R6 sigue abierto para ampliar el corpus y obtener una reducción autorizada.

**Tercer original y carga (10 de noviembre, perfil 1.15.0).** Se evaluó un PDF privado de 575 páginas y 47,65 MiB con OCR previo invisible y capturas de código. La lectura de texto en grupos de ocho páginas amortiza la carga de fuentes; el detector reconoce la transcripción invisible también sobre imágenes envueltas en Forms sin texto, conserva 575 vistas con nota OCR y marca los 37 conceptos como borrador. La conversión por API de pruebas baja de 235,372 a 89,471 s sin elevar límites. Las cinco conversiones finales son válidas y no presentan fallos de salud: dos peticiones con un worker tardan 184,449 s y con dos, 108,228 s; el máximo agregado muestreado de contenedores es 564,65 MiB. La cobertura sigue en 99,76 % de palabras / 99,96 % de texto ajustado, pero el Markdown de código conserva errores OCR e infiere tablas: no supera la fidelidad estructural. Los dos originales anteriores y las 27 muestras públicas conservan su contenido y cobertura. La instancia habitual permanece en 1.14.0; [evidencia y límites](../decisiones/PRUEBAS_PDF_REALES.md#tercer-pdf-real-575-páginas-con-ocr-previo-perfil-1150). R6 continúa para más casos y carga sostenida; no se incorpora contenido privado al corpus público.

**Continuación validada y desplegada (10 de noviembre, perfil 1.16.0).** Se recuperan 780 bloques cercados y se eliminan las tablas inferidas del tercer original, conservando la numeración y los caracteres OCR y midiendo su sangría en la vista. Los 52 marcadores originales se comprueban por página; cinco URI sin esquema se resuelven con un destino inequívoco del mismo PDF y una queda declarada. Una regresión del corpus acotó la unión de marcadores a títulos con prefijo de capítulo y otra conserva filas cuando la numeración OCR es errónea. La carga final completa 12/12 trabajos en 640,356 s, con cuatro solicitudes y dos workers, sin reintentos ni fallos de salud; se registra cada respuesta y ambos relojes para detectar suspensiones. La instancia de 8080 se actualizó tras validar y reconvirtió el original en 90,392 s y 341 MiB; su ZIP coincide con los de la prueba y pasa el validador independiente. Las 136 pruebas, los 27 ejemplos públicos, el smoke y las regresiones privadas pasan. Los errores de reconocimiento originales siguen marcados y respaldados por vistas; la mejora de disposición no afirma código ejecutable. [Evidencia y límites](../decisiones/PRUEBAS_PDF_REALES.md#continuación-del-tercer-pdf-código-navegación-carga-sostenida-y-8080-perfil-1160). R6 continúa para otros diseños y condiciones.

### R7. Cierres documentales (S)

**Problema.** La documentación normativa todavía presenta como vigentes decisiones del plan inicial que el código ya no sigue. Corregirlo no depende de ningún otro trabajo.

**Trabajo.** Aplicar en un solo commit los ajustes de la tabla de §3:

1. [Stack, §1](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md#1-decisión-de-despliegue): un ejecutable con subcomandos `api` y `worker`, y una sola imagen.
2. Stack, §2 y §3: Xberg abandonado, con los motivos de §3, y sin la condición «Reevaluar Xberg si el corpus DOCX muestra…» del párrafo «Lector OOXML propio».
3. [Arquitectura conceptual, §4](../arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md#4-estados-y-garantías) y [arquitectura técnica, §3](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#3-ciclo-de-un-trabajo): estados sin `validating`, con la validación dentro de `processing`.
4. [Arquitectura conceptual, §3](../arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md#3-modelos-de-lectura-y-modelo-de-salida) y stack, §3 (párrafos del modelo intermedio y de los `source_spans`): describir lo que existe hoy, modelos de lectura propios por formato, y anunciar el modelo de salida común de `R1`.
5. La decisión de `R8` sobre la división.

**Aceptación.** Ningún documento normativo describe como vigente una decisión de la tabla de §3; el [estado del proyecto, §4 y §5](../estado/ESTADO_DEL_PROYECTO.md#4-brechas-frente-a-la-arquitectura-y-el-stack) se actualiza en consecuencia.

### R8. División en partes y capítulos (S, o L si se implementa)

**Problema.** La especificación inicial pedía, para PDF y DOCX extensos, índices de partes y capítulos y un `capitulo.md` con el texto introductorio; la regla de unidad semántica habla de «partes, capítulos y secciones reales». El código divide en un solo nivel (`seccion-NNN.md`), que es lo que describen las reglas detalladas de DOCX y PDF de la misma especificación. La especificación se contradice a sí misma y una de sus dos lecturas contradice al código.

**Trabajo.** Decidir y dejar la especificación coherente:

- **Opción recomendada:** ajustar la tabla de la [especificación, §5](../especificacion/ESPECIFICACION_OKF.md#5-reglas-por-formato) y la regla de unidad semántica de la [§4](../especificacion/ESPECIFICACION_OKF.md#4-división-y-fidelidad-comunes) para describir la división en un nivel, y reabrir la división en varios niveles solo si un documento largo de `R6` (libro o tesis) muestra secciones demasiado largas o un índice plano poco navegable.
- **Alternativa:** implementar la división en varios niveles en DOCX y PDF. Es un cambio L que conviene hacer después de `R1`, paso 1, para escribirlo una sola vez.

**Aceptación.** La tabla de la §5 y las reglas detalladas de DOCX y PDF describen la misma división; el límite «División» del [estado del proyecto, §3.3](../estado/ESTADO_DEL_PROYECTO.md#33-límites-conocidos) desaparece o pasa a ser un pendiente explícito. Si `C9` ya está hecho, el cambio sube la versión del perfil.

### R9. Auditoría de dependencias (S)

**Problema.** La regla 4 del stack inicial pedía inspeccionar `cargo tree`, el tamaño del binario, el tiempo de compilación y las licencias de las dependencias transitivas antes de cerrar el stack. Solo se hizo al incorporar `pdf_oxide`.

**Trabajo.**

1. `cargo deny` con un `deny.toml` que admite las licencias ya registradas en el stack (MIT, Apache-2.0, BSD, Zlib, Unicode-3.0, Unlicense) y solo dependencias de crates.io; se ejecuta en la CI con cada push.
2. La comprobación de avisos de seguridad se ejecuta en el flujo programado de `C5`, para que un aviso nuevo no bloquee un push sin relación.
3. Registrar en el stack el número de crates, el tamaño del binario `--release` y el tiempo de compilación limpia en el estado actual.

**Aceptación.** La CI falla si entra una dependencia con una licencia no admitida o de otro origen; el stack tiene las cifras actuales.

## 3. Lo que no se recupera

Estas decisiones del plan inicial se cierran sin implementarse. `R7` aplica los ajustes para que la documentación no las siga presentando como vigentes.

| Decisión inicial | Motivo para no recuperarla | Documento que se ajusta |
| --- | --- | --- |
| Xberg como extractor general | En PDF se midió y se descartó con datos. En OOXML no se midió: adoptarlo ahora exigiría rehacer tres adaptadores que ya dan los localizadores que exige el perfil. La evidencia que lo sustituye es la cobertura de los lectores propios (`R2`). | Stack, §2 y §3. |
| Modelo intermedio común antes del ensamblado | Los cuatro adaptadores ya tienen modelos de lectura propios que conservan los localizadores; unificarlos no aporta lo suficiente frente a unificar la salida (`R1`). | Arquitectura conceptual, §3; stack, §3. |
| LibreOffice para las vistas PPTX | BetterOffice cumple con pérdidas acotadas; se revisa solo si el corpus real (`R6`) muestra tablas o gráficos con pérdidas. | Ninguno: la [decisión de renderizado](../decisiones/RENDERIZADO_PPTX_MEDICIONES.md) ya lo recoge. |
| Dos ejecutables | Un ejecutable con subcomandos da los dos procesos que exige la arquitectura. | Stack, §1. |
| Dos imágenes (API pequeña, worker con renderizado) | Con un único ejecutable, la única diferencia es la fuente TrueType; dos imágenes duplican la construcción sin ahorro apreciable. | Stack, §1. |
| Estado `validating` registrado | La validación ocurre antes de publicar; un estado más no añade garantías. | Arquitectura conceptual, §4; arquitectura técnica, §3. |
| Calamine | El lector propio ya da hoja, rango y lo que Calamine no cubre. | Ninguno: el stack ya lo recoge. |

## 4. Orden

1. `R7` y la decisión de `R8`: solo documentación, sin dependencias.
2. `R4`, `R9` y `R2` para PDF, DOCX y PPTX (sus listas de referencia alimentan `C2`).
3. `R1`, después de `C1`, empezando por las secciones compartidas de DOCX y PDF; `R5` en cuanto `R1` separe las etapas.
4. `R3`, cuando `R4`, `R5` y `R9` estén hechos, para que la imagen incluya la configuración, el registro y las dependencias definitivas.
5. `R6`, de forma continua según lleguen documentos; con el primer libro XLSX real se completa `R2` para XLSX.

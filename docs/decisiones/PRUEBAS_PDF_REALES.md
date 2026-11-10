# Pruebas de calidad y carga con PDF reales

**Fecha:** 9 de noviembre de 2026.

**Origen:** plan 1, R6 (corpus representativo). La primera prueba usa un original privado de `datos_de_prueba/`. Ni el original, ni sus extractos, ni sus listas de palabras se incorporan al repositorio.

## Método repetible

La evaluación separa cuatro propiedades: conformidad del bundle, conservación del texto, estructura editorial y comportamiento bajo carga. El estado `complete` del conversor y una cobertura alta de palabras no demuestran por sí solos que una tabla o un listado de código estén bien reconstruidos.

1. Inventariar páginas, tamaño, capa de texto e imágenes con una herramienta independiente. Renderizar una selección de páginas: portada, índice, texto, tabla, figura, código y página con peor cobertura.
2. Convertir el original sin modificar los límites para hacer que pase. Conservar versión del perfil, revisión del código, parámetros, resultado, duración por etapa, CPU y RSS. Distinguir el tiempo de conversión de la subida, la espera en cola y la descarga.
3. Ejecutar `validate` sobre el ZIP descargado. Comparar con PDFium mediante `tools/pdf_quality.py`, que comprueba que el SHA-256 del PDF recibido coincide con el declarado y con el original incluido, y escribe cifras globales y por página, sin listas de palabras ni extractos.
4. Contrastar visualmente el orden de lectura, títulos, columnas, tablas, figuras, código y procedencia. La métrica por página es conservadora: un bloque que cruza páginas se compara con todo su tramo. La métrica global sigue la normalización y el multiconjunto de [cobertura independiente](COBERTURA_INDEPENDIENTE.md); su ajuste por palabras reagrupadas no es una tasa de exactitud semántica ni OCR.
5. Ejecutar una carga acotada con `tools/stress_api.py`: una solicitud, dos solicitudes simultáneas con un worker y dos con dos workers. Cada worker convierte un trabajo a la vez. El script transmite el original por bloques, sondea el estado, descarga los ZIP, muestrea `/health` y guarda `/metrics` antes y después. La espera hasta observar `processing` tiene la resolución del sondeo (0,5 s por defecto); el registro del worker da la duración exacta del intento y el pico de RSS.
6. Registrar también `health-samples.json` (instante, HTTP, estado de base/almacén/worker y edad del latido), `wall_elapsed_s` y la diferencia con el reloj monotónico. En portátiles se evita el reposo durante la medición; en macOS se puede envolver el comando con `caffeinate -i`, sin cambiar ajustes permanentes. Una suspensión invalida la interpretación como carga continua aunque se completen los trabajos: se conserva la evidencia y se repite la tanda.
7. Conservar los resultados y bundles privados bajo `output/`, que está ignorado. Registrar aquí solo forma, cifras, conclusiones y límites. Probar las correcciones con los ejemplos públicos, sin ajustar sus mínimos para ocultar regresiones.

Ejemplo de auditoría de una conversión ya descargada:

```bash
uv run tools/pdf_quality.py datos_de_prueba/pdf_dificil1.pdf \
  output/calidad-pdf-dificil1/load-single/bundle-001.zip \
  --json output/calidad-pdf-dificil1/final-quality.json
```

Ejemplo de carga contra una **instancia de pruebas con almacén y esquema PostgreSQL propios**, ya arrancada y sin otros trabajos:

```bash
python3 tools/stress_api.py datos_de_prueba/pdf_dificil1.pdf \
  --url http://127.0.0.1:18080 --requests 2 --concurrency 2 \
  --output output/carga-pdf-01
```

El script crea trabajos reales y exige un directorio de salida nuevo. Una espera del cliente agotada no cancela el trabajo del servicio. La carga no cambia automáticamente los límites ni el número de workers. Para validar el resultado:

```bash
target/release/okf-microservice validate output/carga-pdf-01/bundle-001.zip
```

## Primer PDF largo: forma y referencia

Original de 31 483 036 bytes (30,02 MiB), 346 páginas, sin cifrado, con marcadores y numeración impresa. PDFium obtiene 98 493 palabras normalizadas y detecta una página sin texto (la 4). El conversor clasifica 319 páginas como texto digital, 26 como mixtas y una en blanco; no detecta páginas escaneadas. Contiene prosa, índices, diagramas, capturas, tablas y listados de código con anotaciones laterales.

La instancia local que estaba ejecutándose usaba un perfil antiguo (1.0.0), distinto al código del repositorio (1.10.0). Su intento por la API falló a los 165,672 s, con 195 MiB de pico, al confundir una llamada de código con un enlace interno. El diagnóstico guardado en el error del trabajo creció aproximadamente a 13 MB por los mensajes repetidos de las bibliotecas. La API y su salud siguieron disponibles.

La compilación inicial del repositorio (1.10.0), ejecutada directamente en macOS, falló también: una anotación URI sin esquema se emitía como enlace relativo al ZIP. El bundle generado antes del rechazo permitió medir la conservación del texto. Esos tiempos pertenecen a plataformas y configuraciones de registro distintas; no constituyen una comparación de velocidad antes/después.

El código evaluado es el árbol de trabajo con estas correcciones sobre la revisión `31a76800d588c30286fda24fca2164f788d428a1`, todavía sin un commit nuevo. El identificador de revisión del binario no incluye por sí solo los cambios sin commit; el manifiesto local `output/calidad-pdf-dificil1/source-manifest.json` conserva las huellas de los fuentes y herramientas evaluados.

## Correcciones comprobadas (perfil 1.11.0)

- El inventario y el validador de enlaces ignoran el código literal cercado e inline. Una llamada con corchetes y paréntesis no provoca un falso enlace roto. Los enlaces reales que siguen al bloque continúan comprobándose.
- Una anotación URI sin esquema conserva su texto con la marca de referencia no resuelta y se declara. No se inventa un protocolo ni se interpreta la ruta respecto de `contenido/`.
- Los títulos de los marcadores pueden reconstruirse desde hasta cuatro párrafos contiguos de la misma página si el texto completo coincide. También se admite el título exacto sin el prefijo numérico del marcador, únicamente en su página de destino; el número impreso se conserva. Se mantiene el mínimo de cuatro quintos de marcadores encontrados.
- El supervisor drena las salidas del hijo conservando solo los últimos 64 KiB de cada flujo, en vez de acumular todo el diagnóstico en memoria y en la respuesta del trabajo.
- En PDF mayores de 16 MiB se evitan las reaperturas del paso de recursos cuando una página no necesita imágenes nuevas ni vista. Se mantiene la reapertura que limita la acumulación de cachés.

Estas reglas de salida corresponden al perfil **1.11.0**, con su esquema de frontmatter y compatibilidad de validación con 1.0.0–1.10.0. Los 27 bundles de referencia se regeneraron: fuera de `profile_version`, `conversion_id` y `unit_id` no cambian sus textos. La cobertura independiente de los ocho ejemplos PDF conserva sus cifras y mínimos.

## Calidad del bundle final del perfil 1.11.0

| Propiedad | Resultado |
| --- | --- |
| Conformidad | `complete`; ZIP descargado aceptado también por `validate` |
| Páginas representadas | 346 de 346 |
| Palabras conservadas frente a PDFium | 98,66 % (1 323 apariciones sin correspondencia exacta) |
| Texto, con palabras reagrupadas | 99,54 % |
| Conceptos | 21; antes eran 6, con un concepto que abarcaba las páginas 5–342 |
| Marcadores reconocidos | 179 de 214 (83,64 %); antes se descartaba el esquema y se inferían 31 encabezados |
| Imágenes adjuntas | 228, con 65 473 301 bytes sin comprimir dentro del ZIP |
| ZIP descargado en macOS | 90 923 157 bytes (86,71 MiB), incluido el original |

Los once capítulos principales tienen ahora límites de concepto coherentes con sus páginas de comienzo; la organización sigue la jerarquía de los marcadores del original. La recuperación de títulos añade texto a la estructura de `documento.md`, por lo que el número de palabras adicionales aumenta: esa cifra incluye los títulos de navegación y las etiquetas generadas; no demuestra duplicación de párrafos del cuerpo.

### Límites observados en la revisión visual

- **Tablas:** en la página física 85, una tabla visual de tres columnas se reconstruye parcialmente en dos, se mezcla con el encabezado de página y pierde relaciones entre celdas. Que sus palabras sobrevivan no preserva su significado tabular. El contador del informe suma 168 páginas con candidatos tabulares; no identifica 168 tablas verificadas.
- **Código con llamadas laterales:** en las páginas físicas 109–110 el texto monoespaciado queda fragmentado por las anotaciones explicativas y los encabezados. La sangría se reinicia en algunos fragmentos. El contenido no debe tratarse como código ejecutable reconstruido.
- **Encabezados de capítulos:** los que cambian entre capítulos no siempre alcanzan la frecuencia global del 30 % y quedan dentro del contenido; algunos contaminan una tabla o separan un listado. También quedan números de sección separados por la lectura de columnas de margen.
- **Marcadores pendientes:** 35 no se reconocen como encabezado; el texto puede existir dentro de una tabla o separado por la maquetación. No se reduce el mínimo de reconocimiento para ocultarlo.
- **Contraportada:** la página 346 es la peor en la comparación por página (91,74 % de texto ajustado), con tipografías decorativas, letra capitular y columnas. Requiere un tratamiento distinto al de la prosa corriente.
- **Figuras y páginas mixtas:** se conservan imágenes y original, pero no se aplica OCR al texto que está solo dentro de capturas de páginas mixtas. PDFium tampoco lo ve; la cobertura medida no evalúa ese contenido.

La prueba demuestra una mejora de navegabilidad y corrige fallos que impedían entregar el bundle. La fidelidad de tablas complejas y de código con anotaciones sigue siendo un trabajo pendiente; no se declara resuelta por el estado `complete`.

## Carga medida en macOS

API, workers y archivos de prueba aislados del servicio habitual; esquema propio en la base desechable `okf_test`. Cinco conversiones, todas en un intento, `complete`, con validación antes de publicar. Los cinco bundles tienen exactamente las mismas entradas y bytes después de excluir la fecha de generación y la duración del informe.

| Escenario | Tiempo de la tanda, s | Bundles/min | p95 de `/health`, ms | Fallos / sondeos de salud |
| --- | ---: | ---: | ---: | ---: |
| 1 solicitud, 1 worker | 82.59 | 0.73 | 3.68 | 0 / 162 |
| 2 solicitudes simultáneas, 1 worker | 162.04 | 0.74 | 3.58 | 0 / 319 |
| 2 solicitudes simultáneas, 2 workers | 83.26 | 1.44 | 3.18 | 0 / 165 |

Con un worker, el segundo trabajo empezó a observarse en `processing` a los 81,58 s y terminó a los 161,76 s; aumentar el plazo de conversión no reduce esa espera. Con dos workers ambos se observaron en `processing` a los 0,62 s y terminaron a los 82,84 s. Las subidas tardaron 0,106–0,117 s y las descargas de cada ZIP de 86,71 MiB, 0,278–0,417 s, en loopback.

El registro del worker mide conversiones de 80,503–82,446 s, de los que 79,991–81,916 s corresponden al ensamblado (incluye la lectura de páginas PDF) y 0,410–0,427 s a validación. El pico por hijo fue de 293–312 MiB con un worker y 351 MiB en cada conversión simultánea. No se alcanzan los límites de 500 MiB y 300 s.

El muestreo de procesos cada 0,5 s observó hasta 644,97 MiB sumados entre los dos hijos y 681,03 MiB entre API, workers e hijos (sin PostgreSQL ni el sistema). La carga es principalmente de CPU: aproximadamente un núcleo por conversión y dos con dos workers; la suma de `%CPU` llegó a 213,3 % en una muestra. El último tiempo acumulado de CPU observado por hijo estuvo entre 80,66 y 82,29 s; son muestras, no medidas exactas de CPU al terminar. Dos workers casi duplicaron la capacidad para este PDF; eso requiere reservar memoria para ambos hijos y el resto del servicio. No es una recomendación universal de concurrencia.

## Comprobación de la imagen Linux

Se construyó una imagen de prueba separada, `okf-microservice:pdf-quality`, Linux arm64, con las bases y modelos fijados del Dockerfile. Su identidad local es `sha256:df4cdd9308ccb1319e73443da96eadfc87d408aa57496025679ec7b184cc1547`. La comprobación inicial de las correcciones se hizo en instancias de prueba. Después, por petición del usuario, esta misma imagen se promovió a la instancia habitual de Compose en `8080`, conservando sus volúmenes y la imagen anterior con la etiqueta local `okf-microservice:before-profile-1.11-20261109`.

`tools/smoke.sh okf-microservice:pdf-quality` completó el recorrido API → PostgreSQL → worker → descarga → validación de los cuatro formatos y del escaneo con OCR. Sus contenedores, red y volúmenes temporales se eliminaron al terminar.

El PDF privado completo se convirtió además con el subcomando `convert` de esa imagen, **sin red, con un núcleo, cgroup de 600 MiB y los límites del conversor sin aumentar** (500 MiB configurados y 300 s). Se usó un volumen Linux temporal para que Landlock aplicara sus reglas de escritura. Dio `complete` en **86,95 s de pared**, con **202 MiB de RSS máxima** informada por el hijo; lectura 3 ms, ensamblado 86 163 ms y validación 430 ms. El subcomando directo impone Landlock, seccomp y rlimits; no tiene el supervisor de RSS del worker, que sí se ejercita en la prueba de humo. El ZIP recuperado, de 90 923 236 bytes (86,71 MiB), se aceptó también mediante `validate` en otro contenedor sin red; la auditoría PDFium confirmó las mismas cifras globales (98,66 % de palabras y 99,54 % de texto), los mismos 21 tramos de conceptos y 228 imágenes que en macOS. El contenedor y el volumen de esta conversión se eliminaron después de recuperar el resultado.

La cifra de CPU de `/usr/bin/time` aplicada a `docker run` pertenece al cliente Docker, por lo que no se usa como CPU del conversor. La evidencia de CPU por conversión procede del muestreo de los hijos nativos descrito arriba. La latencia Linux es un ensayo con cuota explícita, no una comparación de velocidad con la instancia antigua durante la compilación del host.

## Verificación desde la instancia habitual de 8080 (perfil 1.11.0)

El 9 de noviembre, a petición del usuario, se actualizó la instancia habitual con la imagen ya validada. Se comprobó que no había trabajos `queued` ni `processing`, se conservó la imagen anterior con su etiqueta de recuperación y se promovió la imagen de perfil 1.11.0 a `okf-microservice:local`. Se recrearon únicamente API y worker mediante `docker compose up -d --no-build --no-deps --force-recreate api worker`; PostgreSQL y los volúmenes existentes se conservaron. La identidad de imagen de ambos contenedores coincide con la del apartado anterior. Se mantuvo la configuración habitual, incluido `RUST_LOG=info`.

Se subió de nuevo el original con su nombre real mediante `POST http://127.0.0.1:8080/jobs`, se observó su estado en esa API y se descargó el ZIP publicado por esa instancia. Resultado:

| Comprobación | Resultado |
| --- | --- |
| Estado / intentos / perfil | `complete` / 1 / `1.11.0` |
| Conversión, según el worker | 86.356 s |
| Etapas de lectura / ensamblado / validación | 3 / 85801 / 416 ms |
| Pico residente del hijo | 199 MiB |
| Hasta observar el resultado desde el envío | 87.728 s |
| Subida / descarga | 0.201 / 1.142 s |
| Salud durante la conversión | 0 fallos en 89 sondeos; p95 2.65 ms |
| Bundle descargado | 90,923,236 bytes |
| Validación independiente | Aceptado por `validate` en otro contenedor sin red |
| Auditoría frente al original | 346 páginas, 21 conceptos, 228 imágenes; 98,66 % de palabras y 99,54 % de texto ajustado |

La auditoría confirma el mismo SHA-256 del original, las mismas cifras globales y los mismos tramos de conceptos que en la prueba Linux anterior. Los resultados, la identidad de despliegue, el resumen del worker y el ZIP de esta ejecución se conservan solo en `output/calidad-pdf-dificil1/instance-8080/`. Los límites de tablas y código descritos arriba siguen vigentes.

## Segunda evaluación: tablas y código (perfil 1.12.0)

Esta evaluación corrige los defectos estructurales de la primera. La tabla de la página física 85 se usa para comprobar tres columnas, seis filas de datos y continuaciones de celda. La de la página 251 añade filas altas, etiquetas partidas y parámetros monoespaciados. En ambos casos la referencia se obtiene con PDFium por rectángulos de celda leídos del original; solo se normalizan espacios, escapes Markdown y guiones de maquetación. El verificador privado y sus resultados quedan en `output/calidad-pdf-dificil1/layout-1.12/`, sin incorporar texto del original al repositorio.

### Cambios de maquetación

- Una banda requiere al menos tres filas con tres o más celdas alineadas y canales de anchura comparable. Las columnas se fijan por los inicios de las celdas de dos filas compatibles. Una continuación más larga que el texto usado como anclaje no desplaza el separador hacia la columna vecina.
- Las bandas se identifican antes de agrupar código. Los parámetros monoespaciados de una celda siguen perteneciendo a esa tabla. El espacio vertical distingue las continuaciones de etiquetas de las filas nuevas, y se incluyen todas las líneas finales de cada celda.
- Las líneas de un listado se reúnen por posición, con los espacios interiores y la sangría conservados. Las anotaciones laterales mantienen su texto y procedencia fuera del código. Un listado puede continuar en la página siguiente con un margen físico diferente; sus cajas de origen no se desplazan al normalizar la sangría.
- Los saltos editoriales que parten un literal simple se reúnen bajo las condiciones del perfil y se declaran. Los bloques cercados admiten líneas vacías con el mismo localizador; el validador conserva esa distinción.

La regresión del artículo a dos columnas detectó una confusión entre una tabla de abreviaturas de la columna izquierda y la bibliografía de la derecha. Se añadió una prueba directa que exige mantenerlas separadas, además de conservar los nueve conceptos y las citas del artículo. Otra regresión pública exige que los renglones consecutivos de tarjetas alineadas conserven la lectura por columnas: los dos anclajes iniciales requieren separación vertical entre sus cajas. Las pruebas sintéticas nuevas incluyen continuaciones más anchas que la semilla de columna, etiquetas de varias líneas, filas altas y texto monoespaciado dentro de celdas.

### Medición comparable

La limpieza de Markdown de los medidores Python y Rust también se corrigió: antes interpretaba llamadas de código como enlaces y excluía sus argumentos, o quitaba comentarios literales del código. Para esta comparación se vuelven a medir tanto el ZIP publicado por 1.11.0 como el nuevo con la limpieza corregida. La medición histórica de 1.11.0 de la primera evaluación (98,66 % / 99,54 %) se conserva arriba; con el medidor corregido es **98,68 % de palabras y 99,55 % de texto ajustado**.

La conservación de palabras sigue siendo un multiconjunto, no una prueba de exactitud de tablas ni de validez del código. La comprobación estructural usa celdas concretas y la comparación de caracteres y sangría del listado con las posiciones y fuentes de PDFium. El análisis sintáctico de los dos listados Python se hace con `ast.parse`, sin ejecutarlos ni instalar sus dependencias.

### Resultado final servido por 8080

La imagen final del perfil **1.12.0** está instalada en la API y el worker habituales. Ambos ejecutan la imagen `sha256:5a735d5c5ab13893fbe764efdcae7d273bd017081a715105a67252166b73d5bc`. La conversión verificada corresponde al trabajo `8a373d54-81b6-41d4-b728-a1bc482bf6b3`. El ZIP y las evidencias privadas se conservan en `output/calidad-pdf-dificil1/layout-1.12/verified-8080/`; el manifiesto de fuentes está en `output/calidad-pdf-dificil1/layout-1.12/source-manifest.json`. La compilación final declara `converter_commit: unknown`: el manifiesto registra el commit base y las huellas del árbol sin commit que realmente se compiló.

| Propiedad | Perfil 1.11.0 | Perfil 1.12.0 |
| --- | ---: | ---: |
| Palabras conservadas, medidor corregido | 98,68 % | 98,65 % |
| Texto ajustado, medidor corregido | 99,55 % | 99,53 % |
| Apariciones sin correspondencia exacta | 1 301 | 1 333 |
| Marcadores reconocidos | 179/214 | 199/214 |
| Conceptos / páginas / imágenes | 21 / 346 / 228 | 21 / 346 / 228 |
| Tabla de página 85 | Columnas mezcladas y cabecera incorporada | 7 filas × 3 columnas; 21/21 celdas verificadas |
| Tabla de página 251 | Filas y columnas fragmentadas | 7 filas × 3 columnas; 21/21 celdas verificadas |
| Listado de páginas 109–110 | 10 bloques cercados | Un bloque; 28 líneas no vacías y 3 vacías, con texto y sangría iguales a PDFium |
| Dos listados Python seleccionados | Fragmentación editorial | Un bloque por listado; ambos aceptados por `ast.parse` |
| Intento / estado / validación independiente | 1 / `complete` / válido | 1 / `complete` / válido |
| Conversión en el worker de 8080 | 86,356 s | 85,990 s |
| Pico del hijo | 199 MiB | 202 MiB |

La cobertura global baja ligeramente: los anclajes nuevos reúnen palabras que antes quedaban partidas en celdas y cambian la recuperación de encabezados. Los fragmentos cortos que conserva PDFium no siempre se compensan en el ajuste de palabras reagrupadas, que exige al menos cuatro caracteres. La mejora demostrada aquí es la estructura de las tablas y listados auditados; no se declara una mejora de la cobertura global de palabras.

La última solicitud terminó en 87,713 s desde el comienzo de la subida; la descarga añadió 1,158 s. Hubo **0 fallos en 89 sondeos de salud**, con p95 de 3,184 ms. El intento dedicó 3 ms a lectura, 85 461 ms a ensamblado y 418 ms a validación; el total incluye el resto de operaciones. El ZIP mide 90 921 822 bytes y su SHA-256 es `eb0fa33f0b4e700df600f4ebd79001a76336a3da6ef9eab7c4e418f211d2d22f`. La validación independiente se hizo desde otro contenedor, sin red.

### Verificación del perfil 1.12.0

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` y `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked`: correctos; **122 pruebas**, sin fallos ni omisiones, incluidas OCR y PostgreSQL.
- `cargo deny check`: correcto; sin dependencias ni excepciones nuevas.
- Las 27 referencias golden se regeneraron y revisaron. Además del perfil y sus identificadores, cambian tres archivos de `p1`: la agrupación tabular, sus localizadores y el informe. Los anclajes disjuntos recuperan columnas antes separadas y descartan agrupaciones inestables. Una tabla parcial de cinco columnas pasa a líneas al no disponer de dos filas compatibles; conserva sus palabras, pero su estructura sigue pendiente. Las tarjetas alineadas y la bibliografía conservan la lectura por columnas.
- `uv run tools/coverage.py` sobre los **27 documentos**: coberturas y mínimos sin cambios; no se regeneran listas ni se rebajan umbrales.
- `tools/smoke.sh` con la imagen final: conversiones de los cuatro formatos y PDF escaneado con OCR, con descarga y validación correctas.
- El ZIP final descargado de 8080 se vuelve a auditar contra el SHA-256 del original y con el verificador de 42 celdas y código. Este último compara texto y sangría del listado por posiciones y fuente de caracteres de PDFium; no usa los anclajes del adaptador Rust como referencia.

## Tercera evaluación: márgenes y cabeceras variables (perfil 1.13.0)

**Fecha:** 10 de noviembre de 2026. Las cabeceras corridas de capítulo o sección cambiaban de texto con frecuencia y no alcanzaban el mínimo global del 30 %. Algunas se confundían con el título real de la sección, desplazando el encabezado a la parte superior de la página; el número lateral del título del cuerpo quedaba separado.

La corrección separa cabeceras por su alineación con un folio confirmado y un puesto tipográfico que se repite en al menos tres páginas. Admite prefijos en letra menor, reúne los fragmentos de una fila y registra cada texto una vez en `documento.md`, con todas sus páginas. El título del cuerpo se reúne con su número lateral antes del corte por columnas solo si reproduce el marcador completo en su página de destino. El informe declara las páginas afectadas; se conservan las coordenadas y no se rebajan los mínimos de reconocimiento de marcadores.

### Comprobación independiente

La referencia se extrae del original con PDFium. La inspección visual fija una franja de evaluación de 43 puntos desde el borde superior en las páginas físicas 25–345; esta franja contiene las cabeceras corridas y sus folios, mientras que los títulos y la prosa comienzan debajo. Se obtiene texto de cabecera en **303 páginas**, con el folio separado, y se contrasta su inventario y sus páginas con `documento.md`. También se exige que ningún bloque textual del contenido conserve una caja dentro de esa franja.

Los marcadores leídos con PDFium y la presencia del número en el margen del original dan **152 títulos numerados** para contrastar (144 decimales y ocho con letra de apéndice): el título completo debe aparecer como encabezado en la página de destino y tener su caja en el cuerpo, abarcando el número lateral. En el ZIP de 1.12.0 solo **93/152** cumplían este criterio; había **549 bloques** localizados en la franja de cabecera y ninguna de las 303 cabeceras se registraba íntegra con su página. La referencia geométrica es independiente de las líneas y los cortes del adaptador Rust.

Se repiten además las comprobaciones anteriores de 42 celdas y de los listados, para detectar efectos sobre las tablas, las anotaciones y el código que continúa entre páginas. El verificador y los datos privados se conservan en `output/calidad-pdf-dificil1/margins-1.13/`.

### Cómo comparar la cobertura

La cobertura global sigue comparando todo el texto de PDFium, incluidas las cabeceras repetidas, con el contenido del bundle y el inventario que las registra una vez. Separarlas cambia esa métrica por diseño. Se añade una segunda medición con la misma referencia geométrica para ambos ZIP: las páginas 25–345 se leen debajo de los 43 puntos de cabecera y las demás páginas completas. Esta referencia contiene **96 178 palabras**; el ZIP anterior conserva el **98,65 % de palabras y el 99,56 % de texto ajustado**. La medición del cuerpo complementa el inventario completo de cabeceras y la conservación de `page_labels`; no sustituye la cifra global.

### Resultado final servido por 8080

La API y el worker habituales ejecutan la imagen `sha256:9aeecb584b44f76833e4b5ee2d689714d33e9d772e1575ad47b8a52176d92243`, perfil **1.13.0**. La comprobación final corresponde al trabajo `a6714392-277a-4f46-9527-6b13365731d1`; el ZIP, la identidad de los contenedores y las auditorías quedan en `output/calidad-pdf-dificil1/margins-1.13/verified-8080/`. El manifiesto de fuentes está en `output/calidad-pdf-dificil1/margins-1.13/source-manifest.json`: conserva la revisión base y las huellas del árbol sin commit que se compiló; la revisión del binario no identifica por sí sola esas modificaciones.

| Propiedad | Perfil 1.12.0 | Perfil 1.13.0 |
| --- | ---: | ---: |
| Cabeceras completas registradas con sus páginas | 0/303 | **303/303** |
| Bloques textuales con caja en la franja de cabecera | 549 | **0** |
| Títulos con número lateral y localizador en el cuerpo | 93/152 | **152/152** |
| Marcadores reconocidos | 199/214 | 201/214 |
| Páginas / conceptos / imágenes | 346 / 21 / 228 | 346 / 21 / 228 |
| Etiquetas impresas `page_labels` | 334 | 334, exactamente iguales |
| Cobertura global de palabras (con cabeceras repetidas en la referencia) | 98,65 % | 98,29 % |
| Texto global ajustado | 99,53 % | 99,56 % |
| Cobertura de palabras de la referencia sin cabeceras corridas | 98,65 % | 98,65 % |
| Texto ajustado de esa referencia | 99,56 % | 99,56 % |
| Apariciones sin correspondencia exacta en esa referencia | 1 298 | 1 298 |
| Celdas de las dos tablas comprobadas | 42/42 | 42/42 |
| Listados Python comprobados | Dos válidos por análisis sintáctico | Dos válidos; se mantienen texto, sangría y líneas vacías del listado comparado con PDFium |
| Intento / estado / validación independiente | 1 / `complete` / válido | 1 / `complete` / válido |
| Tiempo del worker / pico del hijo | 85,990 s / 202 MiB | 87,401 s / 202 MiB |

La reducción de la cobertura global de palabras corresponde a retirar repeticiones de cabecera del cuerpo, conservando cada texto con todas sus páginas en el inventario. El control de la referencia sin esas cabeceras mantiene exactamente su número de apariciones ausentes y sus dos porcentajes. El detalle no es idéntico: tres fragmentos breves pasan a palabras reunidas y se recuperan tres números de página; la igualdad del total no implica identidad de las ausencias. El incremento de títulos correctamente localizados se comprueba por texto, página y caja de origen, además del contador de marcadores.

La solicitud final terminó en 87,913 s desde el comienzo de la subida; la descarga añadió 1,111 s. Se registraron **0 fallos en 89 sondeos de salud**, con p95 de 2,927 ms. El intento dedicó 3 ms a lectura, 86 832 ms a ensamblado y 434 ms a validación; el total incluye las demás operaciones. El ZIP mide 90 910 473 bytes y su SHA-256 es `433e2448eb1836b431c2a6276ab648262e6962bcd136f991887e233e8d6ef010`.

### Verificación y límites

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` y `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked`: correctos; **126 pruebas**, sin fallos ni omisiones, incluidas OCR y PostgreSQL. Las regresiones cubren cabeceras variables, versalitas pequeñas, necesidad de folio confirmado y tres páginas, exclusión de código y letra grande, títulos de varias líneas y numeración de apéndices, y rechazo de títulos incompletos, desalineados o de otra página.
- `cargo deny check`: correcto; sin dependencias ni excepciones nuevas.
- Las 27 referencias golden se regeneraron y revisaron. Fuera del perfil y sus identificadores derivados, solo cambia una observación del informe del artículo `p2`; el contenido y los localizadores de todos los ejemplos permanecen iguales a 1.12.0.
- `uv run tools/coverage.py` sobre los 27 ejemplos: cifras en ambos sentidos y mínimos sin cambios; no se regeneran listas ni se rebajan umbrales.
- `tools/smoke.sh` con la imagen final: cuatro formatos y PDF escaneado con OCR, con descarga y validación correctas. El ZIP final de 8080 pasa también `validate` en otro contenedor sin red y las comprobaciones privadas de cabeceras, títulos, tablas y código.

Los casos de cabecera corrida y numeración lateral de este original quedan comprobados. El detector exige folio confirmado y recurrencia de posición, por lo que no cubre cabeceras aisladas, desalineadas con el folio o con otros formatos. Se mantienen los límites de rotación, tablas sin anclajes estables o celdas combinadas, texto solo en capturas de páginas mixtas y tipografías decorativas de la contraportada. Quedan 13 marcadores sin reconocer; no se reduce el umbral del perfil ni se presupone correcta toda la maquetación del libro.

## Parámetros y criterio operativo

Para este documento se conservan los límites por defecto: `OKF_MAX_MEMORY_MB=500`, `OKF_CONVERSION_TIMEOUT_S=300`, `OKF_MAX_PAGES=2000`, `OKF_MAX_IMAGE_PIXELS=33554432`, `OKF_MAX_OUTPUT_MB=512` y `OKF_OCR_ENABLED=false`. No hay páginas sin texto que necesiten OCR; aumentar sus páginas o su memoria no corrige los problemas de maquetación encontrados. Tampoco se suben memoria o plazo para esconder un fallo de validación.

La instancia de pruebas usa `RUST_LOG=okf_microservice=info`, que conserva los registros por intento sin el diagnóstico detallado de todas las fuentes de `pdf_oxide`. La captura acotada protege al supervisor incluso si se habilitan registros más verbosos. Los tiempos locales se miden en macOS arm64 sobre Apple M2 (8 núcleos, 16 GiB de RAM), con PostgreSQL local en Docker; no equivalen a capacidad garantizada del contenedor de producción. Las cifras de salud corresponden solo a esta carga de cinco trabajos, no a una prueba de saturación sostenida.

## Verificación de la primera evaluación (1.11.0)

- `cargo fmt --check` y `cargo clippy --all-targets --locked -- -D warnings`: correctos.
- `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked`: 114 pruebas pasadas, incluidas OCR y PostgreSQL; además se ejecutó la regresión específica de títulos tras ampliar sus casos negativos.
- `cargo deny check`: avisos de seguridad, licencias, orígenes y duplicados aceptados, sin excepciones nuevas.
- `OKF_UPDATE_GOLDEN=1 cargo test --locked golden`: 27 referencias regeneradas y revisadas; solo cambian perfil e identificadores derivados.
- `uv run tools/coverage.py` sobre los ocho PDF de `examples/`: cifras y mínimos sin cambios.
- `validate` independiente sobre los ZIP descargados, comparación de los cinco bundles bajo carga y prueba de humo de la imagen Linux: correctos.

## Próximas pruebas

R6 queda en curso. Las dos tablas y los dos listados seleccionados ya cuentan con comprobaciones estructurales privadas y regresiones públicas o sintéticas de sus mecanismos; falta un caso reducido autorizado que conserve su combinación real de maquetación. No se extrapola la exactitud de esas 42 celdas a todas las tablas del libro. Las siguientes entradas reales deben incluir escaneos degradados, documentos mixtos con texto solo en imágenes, tablas con combinaciones de celdas y listados con otras tipografías o rotaciones. Las cabeceras variables y los títulos laterales de este original quedan comprobados en la tercera evaluación. Falta ampliar los casos a cabeceras sin folio o sin posición recurrente; la contraportada y otros diseños siguen pendientes.


## Segundo PDF real: cuaderno apaisado con formularios (perfil 1.13.0)

**Fecha:** 10 de noviembre de 2026. Original privado de **65 páginas**, 3 207 894 bytes (3,06 MiB), formato 960 × 540 puntos, sin cifrado y con capa de texto en todas las páginas. Combina instrucciones, tablas con reglas, escalas de respuesta, diagramas vectoriales y navegación lateral girada. Contiene 65 marcadores y un AcroForm con 116 campos lógicos (78 de texto y 38 de botón) y 182 widgets. La inspección independiente de campos y widgets no encuentra valores introducidos ni botones seleccionados; por tanto, esta prueba no demuestra conservación de respuestas rellenadas.

La evaluación usa la misma imagen del perfil **1.13.0** ya instalada en 8080, `sha256:9aeecb584b44f76833e4b5ee2d689714d33e9d772e1575ad47b8a52176d92243`. No se cambia el código ni el perfil para esta medición. El manifiesto local comprueba que las huellas de los fuentes coinciden con la compilación auditada anteriormente. Original, texto extraído, renders, verificadores y bundles permanecen bajo `datos_de_prueba/` y `output/calidad-pdf-dificil2/`, fuera del repositorio.

### Conversión real desde 8080

El trabajo `bbd7f30b-6cf0-4e12-b798-287bf8693544` terminó en **un intento, `complete`**, y el ZIP descargado pasó `validate` desde otro contenedor sin red. La auditoría comprueba que el SHA-256 del original coincide con el declarado y con el PDF incluido en el ZIP. Las evidencias están en `output/calidad-pdf-dificil2/instance-8080/`; el ZIP mide 3 408 394 bytes y su SHA-256 es `ec8a195e1128b577f83f3c18c7b936f0345916829a936cfc8ea8fc0879b56054`.

| Propiedad | Resultado |
| --- | ---: |
| Páginas representadas | 65/65: 62 digitales y 3 mixtas |
| Palabras de referencia, PDFium | 11 364 |
| Conservación global de palabras / texto ajustado | 98,76 % / 99,45 % |
| Apariciones sin correspondencia exacta / adicionales | 141 / 5 023 |
| Conceptos | 5; el último abarca páginas 2–65 |
| Encabezados | 100 inferidos por formato; no se usa el esquema de marcadores |
| Imágenes adjuntas / vistas completas de página | 10 / 0 |
| Tiempo del intento / pico RSS del hijo | 599 ms / 70 MiB |
| Etapas: lectura / ensamblado / validación | 1 / 485 / 79 ms |
| Desde comienzo de subida hasta observar `complete` | 2,072 s; descarga adicional de 0,112 s |
| Salud durante esa solicitud | 0 fallos en 3 sondeos; máximo observado 23,889 ms |

El pie de página repetido explica gran parte de la diferencia global. Como control complementario se excluye de la referencia únicamente su rectángulo en las páginas 2–65: x=800–922, últimos 30 puntos de altura. Se mantienen el resto del borde inferior y toda la banda lateral, y se comprueba que las particiones conservan exactamente el multiconjunto original de palabras. La referencia sin ese pie contiene **11 169 palabras**, con **99,81 % de palabras y 99,97 % de texto ajustado** conservados; quedan 21 apariciones sin correspondencia exacta. La portada se compara completa. Este ajuste no evalúa relaciones de tabla ni sustituye la cifra global.

### Fidelidad estructural: no aprobada

El estado técnico `complete` se reproduce, pero el Markdown todavía no conserva correctamente la estructura del cuaderno. Se renderizaron las 65 páginas en hojas de contacto y se inspeccionaron a mayor resolución las páginas 9, 26, 31, 32, 36 y 53. Los controles de tablas usan texto obtenido por PDFium en rectángulos de celda del original; solo se normalizan espacios, puntuación y marcas Markdown para comparar.

| Caso comprobado | Resultado |
| --- | --- |
| Escala de cinco opciones, página 9 | Solo **3/5 etiquetas** aparecen como rótulos completos. Las dos opciones extremas se mezclan entre sí; no se reconstruye la tabla. |
| Tabla de planificación, página 26 | **5/7 descripciones** conservan su texto contiguo. Las otras dos se interrumpen con texto de la columna vecina. |
| Tabla de cuatro parejas, página 36 | Los cuatro rótulos y los comienzos de sus descripciones están presentes, pero el orden agrupa primero los rótulos y después las explicaciones; no mantiene el recorrido por filas. |
| Diagramas de páginas 31, 32, 38 y 53 | No hay una representación gráfica ni una vista adjunta en el Markdown de esos cuatro casos. Sobreviven etiquetas y parte de las explicaciones, pero se pierden las conexiones y relaciones espaciales no expresadas en el texto. El PDF original sí permanece adjunto. |
| Diagrama circular, página 53 | Se genera una tabla espuria que mezcla el rótulo central con texto de un componente. |
| Navegación lateral | **1 292 bloques de texto girado**; 1 283 localizados en la banda derecha y repetidos a lo largo de 64 páginas. La fragmentación añade ruido al contenido. |
| Esquema y división | Se descartan los marcadores con prefijos de diapositiva y se infieren tamaños. La portada se reparte en tres conceptos, un rótulo de un campo acaba como título y las páginas 2–65 se concentran en un concepto. |
| Navegación interna | El original contiene **567 enlaces internos con destino válido**, repartidos entre siete páginas de destino. El bundle marca **63 referencias internas sin resolver**; no conserva correctamente la navegación. |

Más de la mitad de las 5 023 palabras adicionales corresponden a las etiquetas generadas para los fragmentos girados; el resto incluye títulos, navegación y otras marcas del conversor. Esa cifra no prueba por sí sola duplicación de la prosa original, pero el número de bloques y la navegación reiterada empeoran su uso como Markdown de lectura o entrada para RAG. No hay listados de código ni páginas escaneadas que permitan evaluar esas capacidades en este original.

Los controles AcroForm permanecen en el original adjunto; el Markdown no reproduce su funcionamiento editable. Al estar todos vacíos, esta evaluación no atribuye pérdida de respuestas del usuario. La relación entre las opciones de respuesta y las filas sí es parte de la fidelidad estructural que falla en la página 9.

### Carga y reproducibilidad

Se hicieron otras cinco conversiones con la **misma imagen Linux**, en una API, base PostgreSQL y volumen temporales aislados de 8080: una petición con un worker, dos simultáneas con un worker y dos simultáneas con dos workers. Los workers sondean cada 100 ms y el cliente cada 0,1 s; estos tiempos difieren del sondeo habitual de 8080 y no deben compararse directamente con sus 2,072 s observados de extremo a extremo. Se conservaron los ZIP y registros antes de retirar únicamente esos recursos temporales.

| Escenario | Tiempo de la tanda | Intentos, ms | Pico por hijo, MiB | Fallos / sondeos de salud | p95 de salud, ms |
| --- | ---: | --- | --- | ---: | ---: |
| Una solicitud, un worker | 0,853 s | 624 | 71 | 0/9 | 3,397 |
| Dos simultáneas, un worker | 1,353 s | 612; 537 | 71; 69 | 0/13 | 2,645 |
| Dos simultáneas, dos workers | 0,778 s | 530; 527 | 71; 70 | 0/8 | 8,397 |

Las cinco terminan `complete` en un intento y sus ZIP pasan el validador independiente sin red. Subidas con el mismo nombre técnico, sus entradas y bytes son idénticos tras excluir fecha de generación y duración: los defectos de estructura son reproducibles bajo carga, no un efecto de concurrencia. La prueba es una ráfaga breve, con pocos sondeos, no una medida de saturación sostenida ni una garantía de capacidad. No se midió CPU acumulada en esta tanda; el tiempo por etapa y el pico RSS corresponden al hijo, no al conjunto de contenedores.

### Conclusión y correcciones pendientes

**Conformidad del bundle y estabilidad operativa: correctas en esta prueba. Fidelidad estructural a Markdown: insuficiente.** La cobertura alta confirma que casi todo el texto llega, pero no preserva sus relaciones ni permite dar esta conversión por correcta.

Prioridades del plan 1, R6 para este caso:

1. Reconocer los títulos de marcadores exportados como diapositivas y recuperar una división y navegación útiles.
2. Reunir y separar como plantilla la navegación lateral girada, preservando sus destinos sin repetirla en cada página de contenido.
3. Reconstruir las filas de tablas y escalas con texto de varias líneas, sin intercalar columnas ni convertir diagramas en tablas.
4. Conservar una vista o representación explícita de los diagramas vectoriales y de las relaciones del formulario que no se pueden transcribir fielmente.

Los límites por defecto no se alcanzan: 500 MiB y 300 s frente a 70 MiB y 0,599 s en 8080. Aumentarlos o activar OCR no corrige estos defectos. La siguiente medición de un cambio debe repetir este caso y los controles del PDF de 346 páginas; no se debe degradar el caso ya corregido para adaptar el cuaderno. No se incorporan contenido ni listas de palabras privadas al corpus público sin una reducción autorizada.

## Corrección del cuaderno y regresión de ambos PDF (perfil 1.14.0)

El 10 de noviembre se corrigieron los problemas observados del cuaderno y se repitieron **ambas conversiones desde la instancia habitual de 8080**, después de actualizar API y worker. Los originales privados permanecen sin modificar y fuera del repositorio. Esta sección sustituye la conclusión de fidelidad insuficiente del cuaderno para los controles aquí medidos; la evaluación 1.13.0 anterior se conserva como referencia histórica.

### Comportamiento y alcance

El perfil 1.14.0 reconoce la exportación apaisada con marcadores numerados de diapositiva y crea un concepto por página física. Reúne los títulos por columna antes de ordenar el cuerpo; recupera espacios del marcador únicamente si coincide la secuencia normalizada completa de letras y signos, con las equivalencias tipográficas y de mayúsculas del comparador. Los marcadores sin sufijo no inventan un título. La navegación lateral repetida se reconstruye y se inventaría una vez, conservando destinos reales; los destinos explícitos se resuelven por identidad de página en el árbol PDF, evitando interpretar un número de objeto como número de página.

Las tablas de estas exportaciones se reconstruyen solo en regiones confirmadas por reglas horizontales, con columnas de texto y controles, continuaciones dentro de la celda y etiquetas de primera columna combinada repetidas. Fuera de ellas no se convierten etiquetas de diagramas en tablas supuestas. Una vista PNG de cada página conserva las relaciones visuales, acompañada del texto extraído y de una nota generada. Las reglas y umbrales están en la [especificación, §5](../especificacion/ESPECIFICACION_OKF.md#5-reglas-por-formato); no se elevan límites ni se activa OCR para estos originales digitales.

### Comprobación independiente de estructura

| Control | Cuaderno, 1.13.0 | Cuaderno, 1.14.0 |
| --- | ---: | ---: |
| Conceptos | 5; uno reúne páginas 2–65 | **65; uno por página** |
| Títulos de marcadores con sufijo, comprobados por página | Sin división útil por estos títulos | **63/63** |
| Cabeceras de escala completas, página 9 | 3/5 | **5/5**, tabla de 11 filas y 8 columnas |
| Descripciones de planificación reunidas, página 26 | 5/7 | **7/7**, con su columna correspondiente |
| Parejas de etiqueta y descripción, página 36 | Cuatro con orden incorrecto | **4/4 exactas** |
| Referencias internas sin resolver | 63 | **0** |
| Destinos de navegación lateral | Repetidos y fragmentados | **7**, iguales a los destinos válidos del original |
| Bloques girados de navegación lateral | 1 283 | **0**; quedan 9 fragmentos curvos de diagramas |
| Vistas de página | 0 | **65**; presentes los cuatro diagramas seleccionados |
| Tabla espuria entre etiquetas del diagrama de página 53 | Presente | **Ausente** |

Se conservan sin cambios los bytes de las diez imágenes originales extraídas y las 64 etiquetas impresas `page_labels`. La navegación sin anotaciones de enlace de una página particular permanece como texto: no se le inventan destinos. Las vistas de los diagramas de las páginas 31, 32, 38 y 53 se compararon visualmente con el original; las 65 vistas del ZIP final son idénticas a las del pase visual revisado. Los diagramas se conservan como imagen y texto, **no como relaciones semánticas editables**. Los 116 campos vacíos siguen siendo interactivos únicamente en el PDF original adjunto; esta prueba no evalúa formularios rellenados.

El PDF largo de 346 páginas mantiene **exactamente las mismas entradas y contenidos** que el bundle final de 1.13.0, incluidos originales, imágenes, texto y localizadores, al normalizar solo perfil, identificadores derivados, fecha de generación y duración. La comprobación adicional vuelve a verificar **303/303 cabeceras** con sus páginas, cero bloques residuales de cabecera, **152/152 títulos** localizados en el cuerpo, **42/42 celdas** de dos tablas y los dos listados Python: 28 líneas y tres líneas vacías idénticas a PDFium en el listado comparado, con sangría y ambos análisis sintácticos correctos. Se mantienen 21 conceptos y 228 imágenes; no se afirma fidelidad perfecta de las demás páginas.

### Cobertura: separar contenido y plantilla

La referencia PDFium del cuaderno contiene 11 364 palabras, incluidas las repeticiones del lateral y del pie. Al retirar esas repeticiones del flujo y conservar la navegación una vez, la cobertura global de palabras baja de **98,76 % a 85,00 %** y el texto ajustado de **99,45 % a 96,70 %**. Estas cifras no son directamente comparables como medida de pérdida de prosa: cambió el tratamiento de la plantilla. No se cambian umbrales ni listas para ocultar esa diferencia.

Una segunda comparación conserva solo el cuerpo de cada página: excluye el lateral exterior y el pie por sus rectángulos PDFium, y contrasta los conceptos sin las notas ni vistas generadas. La partición se comprueba para que cuerpo y plantilla reconstruyan exactamente las apariciones de palabras del original. Esta referencia contiene **9 061 palabras**, y excluye **2 303 apariciones** de plantilla. Con la misma referencia y el mismo medidor, 1.13.0 obtiene **99,55 % de palabras / 99,82 % de texto ajustado** y 1.14.0 **99,60 % / 99,87 %**; faltan 36 apariciones frente a 41 antes. En una página la cobertura ajustada baja de 100 % a 99,28 % porque dos fragmentos separados por un guion de fin de línea se reúnen correctamente en una palabra: se mantienen las letras y se verifica su celda. Este cambio de tokenización no se interpreta como pérdida de contenido.

El PDF largo conserva **98,29 % de palabras / 99,56 % de texto ajustado** global, y **98,65 % / 99,56 %** con la referencia sin cabeceras corridas, exactamente como en 1.13.0. Las 27 muestras públicas conservan las cifras de cobertura, ausencias y palabras adicionales; sus contenidos golden no cambian, salvo perfil e identificadores derivados.

### Instancia final y recursos medidos

API y worker ejecutan la misma imagen final `sha256:9a51ef3d5383ebd151e9d1cedd65847fa38225060e733139730e23713df11332`. El manifiesto `output/calidad-regresion-1.14/source-manifest.json` registra las huellas de los archivos compilados y la revisión base: hay cambios sin commit y la revisión del binario por sí sola no los identifica. Se conserva la imagen 1.13.0 anterior para reversión; la actualización no cambia PostgreSQL ni sus volúmenes. Se comprobó la cola vacía antes de recrear los dos procesos.

| Medida final de 8080 | PDF largo | Cuaderno |
| --- | ---: | ---: |
| Trabajo | `18fbce9a-57b4-4338-a3b9-40bfeee05f29` | `1f56e394-14ed-4c50-8c28-137f57b0050b` |
| Estado / intento / perfil | `complete` / 1 / 1.14.0 | `complete` / 1 / 1.14.0 |
| Lectura / ensamblado / validación, ms | 3 / 86 594 / 422 | 1 / 2 492 / 55 |
| Total del intento / pico del hijo | **87,147 s / 201 MiB** | **2,580 s / 131 MiB** |
| Desde comienzo de subida hasta observar finalización | 88,854 s | 4,057 s |
| Descarga adicional | 1,079 s | 0,181 s |
| ZIP, bytes | 90 910 465 | 13 358 558 |
| Fallos / sondeos de salud | **0/90** | **0/5** |
| p95 de salud | 5,360 ms | 21,029 ms |

Los ZIP descargados pasan `validate` en contenedores independientes sin red. Sus SHA-256 son `27a7f3296f7ac4b1d35f174d1102fc06f914ff8ffa0941ba833d15e004d4b13a` (largo) y `46c77af8add4a906084b3632a6fa9038a35748236c7583fe6091ac8cccd6b8f9` (cuaderno). Peticiones, respuestas, sondeos, auditorías y métricas quedan en `output/calidad-regresion-1.14/pdf1-8080/` y `pdf2-8080/`, ignorados por Git. Los pases intermedios se conservan separados.

El cuaderno pasa de 0,599 s / 70 MiB y 3 408 394 bytes a 2,580 s / 131 MiB y 13 358 558 bytes por la extracción adicional y, principalmente, sus 65 vistas. Sigue dentro de los límites existentes de 300 s y 500 MiB. El PDF largo mantiene un consumo equivalente al de su control anterior (87,401 s / 202 MiB). Estos dos trabajos finales fueron **secuenciales**: comprueban calidad y regresión operativa, pero no vuelven a medir saturación ni concurrencia con el coste nuevo de las vistas. El p95 del cuaderno solo tiene cinco muestras; no demuestra un cambio sostenido de latencia. No se midió CPU acumulada de estos dos intentos.

### Verificación y trabajo pendiente

- `cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings` y `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked`: correctos, **131 pruebas sin fallos ni omisiones**, con PostgreSQL y modelos OCR. Incluyen destinos por referencia de página, tablas con reglas, títulos envueltos, recuperación de espacios sin sustituir letras, navegación y vistas en una exportación sintética.
- `cargo deny check`: correcto; no se cambian dependencias ni excepciones.
- Referencias golden regeneradas y revisadas; cobertura independiente de los 27 ejemplos sin cambios, sin rebajar mínimos.
- `tools/smoke.sh` con la imagen final: los cuatro formatos y un PDF escaneado con OCR descargados y validados correctamente.

Los controles seleccionados del cuaderno quedan corregidos y el PDF largo no presenta regresiones de contenido. Permanecen los límites de otros exportadores, tablas sin reglas estables, relaciones de diagramas que solo conserva la vista y campos interactivos en Markdown. El plan 1, R6 continúa para ampliar el corpus, medir carga con esta ruta y obtener una reducción autorizada que reúna estas estructuras sin contenido privado.

## Tercer PDF real: 575 páginas con OCR previo (perfil 1.15.0)

**10 de noviembre de 2026; plan 1, R6.** Se evaluó un original privado de **49 964 337 bytes (47,65 MiB), 575 páginas**, sin cifrado ni formulario, con 52 marcadores, 1 093 objetos de imagen y texto extraíble en todas las páginas. PDFium obtiene 709 325 caracteres y 113 896 palabras normalizadas. El original combina imágenes de las páginas y capturas de código con una transcripción OCR previa; no es una prueba de exactitud del OCR local del servicio. No se publican su título, extractos, capturas ni listas de palabras.

Original, renders, bundles, verificadores y medidas permanecen en `datos_de_prueba/` y `output/calidad-pdf-dificil3/`, ignorados por Git. El manifiesto local conserva la revisión base, las huellas de los fuentes y herramientas y la identidad de la imagen; distingue los fuentes medidos del árbol final, que solo añade una corrección de comentarios sobre la caché. Se evalúa el árbol de trabajo con cambios sin commit; la imagen de pruebas declara `converter_commit: unknown`, por lo que esa etiqueta por sí sola no identifica el código evaluado. **Durante esa evaluación la instancia habitual de 8080 conservó el perfil 1.14.0**: esa prueba utilizó API, PostgreSQL, red y volumen temporales propios en el puerto 18083.

### Referencia inicial y cambios medidos

Con el perfil 1.14.0, la solicitud inicial termina en un intento, `complete`, y pasa la validación. Tarda **235,372 s**, con **245 MiB** de pico del hijo: lectura 4 ms, ensamblado 234 139 ms y validación 1 044 ms. El cliente observa la finalización a los 236,132 s y descarga el ZIP en otros 0,497 s. La salud registra 0 fallos en 467 sondeos, con p95 de 2,641 ms.

Sin embargo, clasifica las 575 páginas como mixtas, no adjunta vistas y no señala que sus conceptos contienen texto OCR sin verificar. La fuente tiene glifos normales, pero el texto se dibuja exclusivamente con `3 Tr` (invisible); las imágenes están envueltas en Form XObjects. El detector anterior solo reconocía una fuente sin glifos. La cobertura frente al texto extraíble no detecta esta diferencia ni los errores que ya contiene esa capa.

Se aplicaron dos cambios generales:

1. **Lectura en grupos de ocho páginas para PDF mayores de 16 MiB.** Se reutilizan fuentes y objetos durante ese grupo y se libera la caché antes de abrir el siguiente. La codificación de imágenes y el renderizado siguen reabriendo por página para acotar su memoria. El sondeo de la biblioteca aislada confirmó el coste de cargar las fuentes: 373 ms en el primer acceso y alrededor de 1 ms en páginas siguientes del mismo documento. En macOS, cambiar solo el grupo reduce la conversión de **223,62 a 38,49 s**; las **1 126 entradas del ZIP son idénticas** al normalizar únicamente fecha de generación y duración. No se elevan los límites de tiempo, memoria, páginas ni imágenes.
2. **Reconocimiento de OCR previo invisible, perfil 1.15.0.** Se comprueban los operadores reales, las cadenas y comentarios y los estados `q`/`Q`. Los Forms que envuelven imágenes se inspeccionan también: no deben contener operadores de texto; se admiten menos de ocho niveles y hasta 64 recursos por página. Si no se pueden comprobar, contienen texto o hay imágenes inline, no se infiere OCR por esta señal. Se conservan las reglas anteriores para fuentes sin glifos. El resultado usa la transcripción existente sin corregirla por conjetura, marca los conceptos como borrador y adjunta una vista y su nota por página. El esquema 1.15.0 conserva la forma del frontmatter; se mantiene la compatibilidad con 1.0.0–1.14.0.

### Calidad del ZIP final descargado

| Control | Resultado |
| --- | ---: |
| Estado técnico / intentos / validación independiente | `complete` / 1 / correcto |
| Páginas representadas y clasificadas como OCR previo | **575/575** |
| Conceptos / conceptos con `status: draft` | **37/37** |
| Vistas, enlazadas una vez con localizador y nota OCR | **575/575** |
| Marcadores reconocidos por el conversor | 49/52 |
| Conservación global de palabras / texto ajustado | **99,76 % / 99,96 %** |
| Apariciones sin correspondencia exacta | 269; antes 268 |
| Tablas inferidas / bloques de código cercados | **315 / 0** |
| Referencias internas sin resolver | 6, marcadas y declaradas |
| ZIP final, incluido el original | **212 456 852 bytes (202,61 MiB)** |

La cobertura conserva las cifras redondeadas de la referencia inicial. Las palabras adicionales pasan de 6 837 a 19 887, principalmente por las notas de incertidumbre OCR que ahora se repiten en su posición; no se presentan como texto del original. El ajuste por palabras reagrupadas mide conservación textual, no exactitud de reconocimiento ni corrección de código. Los 37 cuerpos de concepto mantienen las mismas líneas de texto extraído al retirar imágenes, notas generadas y líneas vacías; una separación en blanco cambia junto a una nota de página. La diferencia de una aparición sin correspondencia se debe a que cambian los rótulos de las imágenes por los de las vistas, no a una transcripción nueva.

Se revisaron visualmente portada, prosa y capturas en las páginas 1, 20, 50, 100, 161, 177, 197, 300, 400, 497, 500 y 575. Las 575 vistas descargadas se compararon además con un render PDFium independiente, reduciendo ambas imágenes a gris de 128×128 píxeles: error absoluto medio **0,00324** y máximo **0,00937**, en escala 0–1. Es un control de representación visual y páginas en blanco, no una medida de legibilidad carácter por carácter. Las vistas conservan los listados visibles; sustituyen las imágenes de contenido sueltas, mientras una imagen repetida de plantilla sigue inventariada una vez y el PDF original permanece adjunto.

**El Markdown de código no supera una evaluación de fidelidad estructural.** El original contiene errores OCR en identificadores, signos y espacios; en los casos revisados se ven caracteres cuadrados en la transcripción de capturas. La geometría y las fuentes de esa capa hacen que el adaptador infiera tablas donde hay listados con números de línea, y no conserva el código como bloques cercados con sangría ejecutable. No se ejecutó código del documento ni se corrigió su significado automáticamente. La corrección entrega la evidencia visual y declara la incertidumbre; no convierte esas 315 tablas en código correcto. Quedan también seis referencias sin resolver y tres marcadores no reconocidos.

### Carga, latencia y recursos

Todas las tandas finales utilizan la misma imagen Linux y los mismos límites por defecto: **300 s por intento, 500 MiB por hijo, 512 MiB por archivo de salida, 2 000 páginas, 100 MiB de subida y OCR local desactivado**. El worker sondea cada 100 ms y el cliente cada 0,5 s. Los contenedores no tienen cuota explícita de CPU ni memoria; Docker Desktop dispone de aproximadamente 7,75 GiB, compartidos con la instancia habitual inactiva. Son ráfagas locales de cinco conversiones, no una prueba de saturación sostenida.

| Escenario | Tanda con descargas, s | Intentos, s | Pico por hijo, MiB | Fallos / sondeos de salud | p95 salud, ms |
| --- | ---: | --- | --- | ---: | ---: |
| Una solicitud, un worker | 93,083 | 89,471 | 306 | 0/184 | 2,797 |
| Dos simultáneas, un worker | 184,449 | 90,091; 90,492 | 304; 306 | 0/366 | 2,724 |
| Dos simultáneas, dos workers | 108,228 | 102,369; 103,672 | 305; 304 | 0/214 | 3,917 |

Con un worker, la segunda solicitud se observa en `processing` a los **91,442 s**, frente a los 0,977 s de la primera en empezar; ampliar el plazo del intento no elimina esa espera. Con dos workers ambas se observan procesando a los **0,963 s** y terminan a los 103,731 y 104,736 s desde el comienzo de sus subidas. La tanda baja de **184,449 a 108,228 s** (aproximadamente 41 %); la capacidad observada pasa de 0,651 a 1,109 trabajos/minuto. Es la medición de una sola ráfaga de este original, no un rendimiento garantizado.

La primera conversión final dedica 6 ms a lectura, 88 750 ms a ensamblado y 552 ms a validación: **89,471 s y 306 MiB**. Frente a la inicial se reduce el intento aproximadamente un **62 %**, aun incorporando las vistas. El ZIP crece de **29 145 645 a 212 456 852 bytes** y su descarga pasa de 0,497 a 3,127 s en loopback: preservar la evidencia visual tiene un coste importante de almacenamiento y red. El pase nativo final tardó 112,91 s, con 103,25 s de CPU de usuario y 1,20 s de sistema, mientras había compilaciones y regresiones en el host; no se usa para comparar velocidad con la imagen Linux.

El muestreo de `docker top` y `docker stats`, aproximadamente cada dos segundos, observa **329,56 MiB** sumados entre worker, API y PostgreSQL en la tanda individual, y **564,65 MiB** entre ambos workers, API y PostgreSQL en la concurrente. Es la suma de memoria de trabajo que presenta Docker, no el RSS exacto agregado de los hijos ni el consumo de todo Docker Desktop. Los máximos por hijo de la tabla proceden del supervisor. El último tiempo de CPU observado por hijo es 88 s en la tanda individual, 90 y 88 s en la cola y 103 y 101 s en paralelo; son cotas inferiores muestreadas y redondeadas. El máximo observado de CPU de los workers pasa de 100,40 % a 207,51 %, aproximadamente uno y dos núcleos. Para dimensionar dos workers deben contemplarse los dos presupuestos configurados de 500 MiB y los procesos auxiliares, además del aumento del tamaño de los bundles.

Los cinco ZIP finales se validan en contenedores independientes sin red y montajes de solo lectura. Al excluir solamente fecha de generación y duración del informe, sus entradas y bytes son iguales; el tamaño comprimido puede variar unos bytes por esos campos. La auditoría comprueba la huella SHA-256 del original declarado y del PDF adjunto. Peticiones, tiempos, muestras, métricas, registros y huellas permanecen en el directorio privado de resultados. Los recursos Docker temporales se retiran después de conservar las evidencias.

### Regresión, verificación y decisión

- Los dos originales privados anteriores mantienen las mismas entradas y contenido que la salida final de 1.14.0, incluidos los recursos gráficos, al normalizar perfil, identificadores derivados, fecha, duración y el nombre técnico de subida cuando difiere. La comparación del cuaderno usa el ZIP final de 8080, posterior al ajuste de espacios en títulos; el pase nativo anterior a ese ajuste no sirve como referencia final. La presentación privada grande también termina dentro del límite de memoria: 330 MiB.
- Las **27 muestras públicas** conservan sus cifras de cobertura, apariciones ausentes y adicionales; las **291 entradas Markdown golden** solo cambian en versión e identificadores derivados. No se rebajan mínimos ni se cambian listas de referencia.
- `cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings`, `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked` con modelos locales y PostgreSQL de pruebas, y `cargo deny check`: correctos. **133 pruebas sin fallos ni omisiones**; se comprueba texto invisible, operadores reales frente a cadenas/comentarios, restauración de estado, fuente normal, imagen directa, imagen envuelta en Form y rechazo de la inferencia cuando el Form contiene texto.
- `tools/smoke.sh` con la imagen final: conversiones y descargas de los cuatro formatos, y PDF escaneado con OCR activo, aceptados por el validador.

**Decisión:** mantener los límites existentes y el grupo de ocho páginas. Dos workers son una opción medida para esta ráfaga, con el coste de memoria y descarga registrado arriba; no se cambia la concurrencia del servicio habitual. La fidelidad visual y la declaración de OCR quedan verificadas; la extracción de código desde OCR previo sigue pendiente. R6 continúa para ampliar casos, reducir regresiones sin contenido privado y medir carga sostenida y control de admisión antes de atribuir una capacidad de producción al microservicio.

## Continuación del tercer PDF: código, navegación, carga sostenida y 8080 (perfil 1.16.0)

**10 de noviembre de 2026; plan 1, R6.** A petición del usuario se continuó la prueba del original privado de 575 páginas. Se corrigió la disposición del código OCR, se auditaron referencias y marcadores, se ejecutó carga sostenida y se actualizó la instancia habitual tras validar el resultado. Los originales, extractos, capturas y ZIP permanecen fuera de Git, bajo `datos_de_prueba/` y `output/calidad-pdf-dificil3-1.16/`. El manifiesto local registra la revisión base, las huellas de los fuentes y la imagen exacta; el árbol contiene cambios sin commit, por lo que el SHA de revisión del binario no identifica por sí solo todo el código.

### Markdown del código y alcance de la corrección

`src/pdf/code.rs` reconoce paneles de fondo claro coloreado con margen numerado y texto de hasta 10 puntos o inferior al 85 % de la prosa de referencia de la página. La primera condición permite las páginas dominadas por código, sin prosa suficiente para estimar esa referencia. Reúne todos los renglones dentro de los límites visuales del panel, sin recortar el principio por el valor reconocido de su numeración, y contrasta la sangría con el comienzo de la tinta en la vista. Conserva los caracteres reconocidos y la numeración, incluso cuando un número fue reconocido como una letra. No carga modelos OCR nuevos, no corrige palabras por conjetura y no declara el programa ejecutable.

| Propiedad | Perfil 1.15.0 | Perfil 1.16.0 final |
| --- | ---: | ---: |
| Bloques cercados | 0 | **780** |
| Tablas inferidas | 315 | **0** |
| Conceptos | 37 | **38** |
| Conceptos en borrador / vistas localizadas con nota OCR | 37 / 575 | **38 / 575** |
| Marcadores originales reconocidos en su página | 49/52 | **52/52** |
| Referencias sin resolver | 6 | **1**, declarada |
| Palabras frente a PDFium | 99,76 % | **99,76 %** |
| Texto ajustado frente a PDFium | 99,96 % | **99,96 %** |
| Apariciones sin correspondencia exacta | 269 | **270** |

Se comprobaron los dos listados de la página 300 contra su vista: cinco y siete renglones, con columnas de sangría `[0, 0, 4, 0, 4]` y `[0, 0, 4, 0, 4, 0, 4]` después del margen numérico. El listado de la página 50 mantiene sus doce renglones, incluidos los numerados sin instrucción; la página 279 tiene una regresión específica para conservar las filas iniciales aunque el OCR confunda un número del final con el primero; la página 100 conserva por separado el ejemplo y su salida. También se revisaron las páginas 53, 71, 161, 177, 197, 497, 541 y 542: las siete tablas residuales del primer candidato eran capturas en páginas dominadas por código y quedan dentro de cercas con la regla final. Las 575 vistas y el resto de recursos gráficos son idénticos byte a byte a los ya contrastados con PDFium; todas conservan una nota OCR y un localizador por página.

**Límite de la capa original:** permanecen errores de identificadores, signos y espacios. En una captura larga hay renglones con coordenadas OCR defectuosas y partes del programa no recuperables desde esa transcripción; la vista conserva el contenido visible. Las cercas mejoran la disposición del texto disponible y evitan inventar relaciones de tabla; no demuestran exactitud carácter por carácter, sintaxis válida ni equivalencia de ejecución. La numeración se conserva dentro de la cerca como transcripción, separada visualmente de la instrucción. Los 38 conceptos siguen siendo borradores por la procedencia OCR.

La cobertura redondeada de palabras se mantiene en 99,76 % y la ajustada, en 99,96 %. Quedan 270 apariciones sin coincidencia exacta frente a 269 antes; la reconstrucción de renglones y márgenes y la reposición de asteriscos cambian la delimitación de algunos lexemas. No se alteran listas ni mínimos. Una auditoría adicional por página detectó un recorte de filas al interpretar una numeración OCR errónea; se eliminó ese recorte, se añadió su regresión sintética y se verificó que la página 279 y el otro borde afectado conservan las letras de los bloques originales. Esta cobertura compara con la capa OCR del PDF, no con una transcripción humana de sus imágenes.

### Marcadores y referencias

Se reúnen dos parejas de entradas consecutivas del esquema cuando la primera tiene prefijo decimal de capítulo «N - título», ambas apuntan a la misma página y profundidad y el título concatenado coincide con un mismo párrafo de esa página. Los componentes numéricos deben ser menores de 100. El requisito evita reunir encabezados independientes de una presentación: la regresión pública detectó esa posibilidad y se corrigió antes de promover la imagen. El informe cuenta las 52 entradas originales aunque sus fragmentos formen 50 encabezados. Un título adicional recupera la posición de los asteriscos desde su marcador, sin cambiar letras, demás signos ni cantidad de asteriscos, en una única línea en negrita de su página. La auditoría independiente con PDFium verifica **52/52 entradas originales**, su título individual o compuesto y su página; ninguna queda ausente.

Cinco de las seis referencias sin resolver eran URI sin esquema para las que el mismo PDF contiene un destino HTTP(S) completo e inequívoco. Se resuelven con esa evidencia local, sin consultar la red. Solo se normalizan mayúsculas y `www.` del dominio y la barra final de la raíz; otras rutas deben coincidir literalmente. La sexta URI, en la página 128, carece de protocolo y de destino absoluto equivalente en el documento: **se conserva marcada y declarada**. La revisión queda cerrada sin inventar su destino. Los URI absolutos del original que contienen cadenas extrañas tampoco se reescriben por conjetura.

### Carga sostenida de la imagen final

Una tanda previa de la imagen final completó los doce trabajos y produjo ZIP válidos, pero registró **95 fallos de salud**. Los eventos de `pmset` confirmaron repetidos reposos y despertares del Mac, y el observador presentó huecos de 23 a 446 segundos. El reloj monotónico del cliente no contabilizó esas suspensiones. Esa tanda no se usa como evidencia de carga continua ni se ocultan sus fallos: sus resultados se conservan aparte. Se repite con una inhibición temporal del reposo automático limitada a esta tarea, sin cambiar la configuración permanente de energía. La herramienta ahora conserva cada respuesta de salud y ambos relojes.

La tanda decisiva usa **12 solicitudes, concurrencia de cliente 4 y dos workers** en API, PostgreSQL, red y volumen propios del puerto 18083. Se mantienen los límites de **300 s, 500 MiB por hijo, 512 MiB por archivo de salida, 2 000 páginas y 100 MiB de entrada**, con OCR local desactivado, sondeo del worker de 100 ms y del cliente de 0,5 s. No se añaden cuotas de CPU o memoria al contenedor; las medidas corresponden a Docker Desktop local. El cliente transmite y descarga los archivos por bloques.

| Medida final | Resultado |
| --- | ---: |
| Solicitudes / concurrencia / workers | 12 / 4 / 2 |
| Duración monotónica / reloj de pared | **640,356 / 640,357 s** |
| Diferencia entre relojes | 1,509 ms |
| Intentos / reintentos / fallos | **12 / 0 / 0** |
| Intento mínimo / mediano / máximo | 98,623 / 104,527 / 114,708 s |
| Salud: fallos / muestras | **0 / 1265** |
| p95 de salud | **5,117 ms** |
| Edad máxima del latido del worker | 10 s, límite de 30 s |
| Máximos observados de procesamiento / cola | 2 / 2 |
| Espera máxima observada hasta procesar, sin subida | 109,654 s |
| Trabajo observado por minuto | 1,124 |

La tanda dura **10,67 minutos continuos**. La diferencia entre relojes y el muestreo de aproximadamente dos segundos, con hueco máximo de 2,198 s, permiten descartar las suspensiones que invalidaron el ensayo anterior. Los sondeos tienen resolución de 0,5 s; la duración exacta del intento procede del supervisor. Hay dos trabajos activos y hasta dos en espera, sin crecimiento ilimitado de la cola en esta prueba acotada.

El supervisor registra entre **341 y 344 MiB por hijo**, dentro de los 500 MiB configurados. El máximo sumado de memoria de trabajo que presenta Docker para ambos workers, API y PostgreSQL es **695,74 MiB**; no es el RSS exacto del conjunto ni el consumo de todo Docker Desktop. El máximo muestreado de CPU de ambos workers es 225,30 %, aproximadamente dos núcleos. No se observó un aumento progresivo del pico por hijo en los doce intentos, lo que no demuestra ausencia de fugas a plazos mayores. Los ZIP miden 212 412 902–212 412 907 bytes y tienen 619 entradas. El cliente descargó y validó los doce; las muestras de procesos, cola, salud, métricas y huellas quedan en el directorio privado.

Cada ZIP descargado se valida en un contenedor independiente sin red y con el directorio montado en solo lectura. Se comparan todas sus entradas por SHA-256, normalizando solo la fecha de generación y duración del informe: **los doce son idénticos en contenido**. El original declarado y el PDF adjunto coinciden con el SHA-256 del original privado. Los registros y auditorías de los ensayos intermedios se conservan aparte y no se mezclan con los resultados finales.

La salud bajo esta carga y la estabilidad observada no fijan una capacidad universal ni descartan fugas a plazos mayores. No se han simulado clientes remotos, caídas de PostgreSQL, reinicios en mitad de esta tanda ni más de cuatro solicitudes activas. La configuración habitual mantiene un worker; la comparación previa con dos justifica evaluar su escalado con presupuesto de memoria, CPU, disco y red explícito. No se cambia automáticamente la concurrencia del servicio habitual.

### Regresión y actualización de 8080

Los 27 ejemplos públicos conservan exactamente las cifras de cobertura, ausencias y palabras adicionales. Las 291 entradas Markdown golden solo cambian en perfil e identificadores derivados. Los dos originales privados anteriores mantienen todas sus entradas y contenido al normalizar perfil, identificadores, fecha y duración; la presentación privada grande también convierte dentro del límite de memoria. No se añaden dependencias ni excepciones de auditoría.

`cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings`, `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked`, cobertura independiente y `cargo deny check`: correctos. **136 pruebas sin fallos ni omisiones**, con modelos y PostgreSQL de pruebas. Incluyen panel con OCR sin sangría pero tinta sangrada, fondo blanco rechazado, página sin prosa de referencia, URI con evidencia y URI ambiguas, títulos fragmentados, rechazo de títulos independientes y recuperación limitada de asteriscos. `tools/smoke.sh` acepta conversiones y descargas de los cuatro formatos y del PDF escaneado con OCR activo en la imagen final.

Se comprobó la cola habitual vacía antes de desplegar. Se conservó la imagen 1.14.0 anterior como `okf-microservice:rollback-pdf3-116-20261110` y se promovió la imagen **`sha256:d4d7c87c07b231313418182101b242dacd754bf88afa266bb898f58b95c49313`** a `okf-microservice:local`. Se recrearon únicamente API y worker mediante `docker compose up -d --no-build --no-deps --force-recreate api worker`; PostgreSQL, volúmenes, configuración y el número habitual de workers se conservaron.

La comprobación real de 8080 publica el trabajo `bc044586-6e42-4696-ac96-fa97149fbfdb` como **`complete`, intento 1, perfil 1.16.0**. El intento tarda **90,392 s**, con **341 MiB**: lectura 3 ms, ensamblado 89702 ms y validación 515 ms. El cliente observa la finalización a los 90,925 s y descarga en otros 2,543 s; la tanda completa dura 93,472 s. La salud registra **0/185 fallos** y p95 de 2,665 ms. API y worker ejecutan la misma imagen validada y la salud final es `ok`.

El ZIP de 8080 pasa el validador independiente sin red y sus **619 entradas coinciden exactamente** con las de la carga al normalizar solo fecha y duración. Su SHA-256 es `13eee8e2968340aac2f7a56207d9e6206b65586f9515f491dc21aa3bd0dff909`. Está en `output/calidad-pdf-dificil3-1.16/instance-8080/bundle-001.zip`, junto con la respuesta, sondeos, auditorías y registro del worker. Los contenedores, volumen y red de prueba se retiraron después de conservar las evidencias; la inhibición temporal del reposo se liberó al terminar la verificación.

**Resultado:** se completa la mejora de disposición del código disponible, la revisión de navegación y la prueba de carga sostenida de este original. La instancia habitual queda actualizada con la imagen validada. Los errores y vacíos de la transcripción OCR original permanecen señalados y respaldados por sus vistas; no se presentan como código verificado. R6 continúa para ampliar el corpus y cubrir otros diseños y condiciones de operación.

### Ajuste posterior de Swagger y conservación de evidencias

El 10 de noviembre se actualizó Swagger para que `document_key` y `access` aparezcan vacíos y **Send empty value** desmarcado inicialmente. El formulario explica que solo `file` es obligatorio. La comprobación real desde el navegador en 8080 seleccionó únicamente un PDF de ejemplo, recibió `202` y terminó como `complete`, con ambos metadatos ausentes y ZIP aceptado por el validador independiente. Formato, Clippy y las 136 pruebas pasaron. La imagen operativa posterior es `sha256:382e2860374610048083206f21709be857f9f1e22e68f6688ed374dafa561060`; el perfil sigue siendo 1.16.0. El manifiesto y las evidencias están en `output/swagger-opcionales/`.

A petición del usuario se limpiaron compilaciones y copias redundantes. Se retiraron los doce ZIP equivalentes de la carga final, los ZIP de cinco tandas intermedias y cinco duplicados exactos del primer PDF. Se conservaron sus huellas, validaciones, registros y métricas, los originales y el ZIP final de 8080 citado arriba. `output/limpieza-20261110/manifest.json` identifica cada archivo retirado y la razón; las imágenes activa y de reversión y los volúmenes persistentes se conservaron. Las afirmaciones de equivalencia describen las comparaciones realizadas antes de retirar esas copias.

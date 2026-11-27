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

## Primer PDF largo: tres evaluaciones (perfiles 1.11.0 a 1.13.0)

Resumen de las tres primeras evaluaciones, que mejoraron el mismo original. Sus cifras y la comparación completa están en la [calidad del Markdown 1.17.0](#calidad-del-markdown-pdf-perfil-1170) y en las secciones siguientes; los resultados privados de cada tanda se conservan fuera de Git en `output/calidad-pdf-dificil1/` (`layout-1.12/` y `margins-1.13/`).

**Forma del original.** 31 483 036 bytes (30,02 MiB), 346 páginas, sin cifrado, con marcadores y numeración impresa. PDFium obtiene 98 493 palabras normalizadas. El conversor clasifica 319 páginas como texto digital, 26 como mixtas y una en blanco, y no detecta escaneadas. Contiene prosa, índices, diagramas, capturas, tablas y listados de código con anotaciones laterales.

**Perfil 1.11.0: de no entregar bundle a `complete`.** La instancia local usaba el perfil 1.0.0 y falló a los 165,672 s al confundir una llamada de código con un enlace interno; el diagnóstico guardado en el error creció a unos 13 MB. Con el repositorio (1.10.0) falló también una anotación URI sin esquema, que se emitía como enlace relativo. Las correcciones:

- El inventario y el validador de enlaces ignoran el código literal cercado e inline.
- Una anotación URI sin esquema conserva su texto con la marca de referencia no resuelta y se declara; no se inventa un protocolo.
- Los títulos de los marcadores se reconstruyen desde hasta cuatro párrafos contiguos de la misma página, o por su título exacto sin prefijo numérico en la página de destino. El mínimo de cuatro quintos de marcadores encontrados no se rebaja.
- El supervisor conserva solo los últimos 64 KiB de cada salida del hijo.
- En PDF mayores de 16 MiB se evitan las reaperturas del paso de recursos que no hacen falta.

**Perfil 1.12.0: tablas y código.** Las bandas de tabla exigen tres filas con tres o más celdas alineadas, y las columnas se fijan por los inicios de dos filas compatibles; las continuaciones de celda y las etiquetas de varias líneas se reúnen. Los listados de código conservan espacios, sangría y líneas vacías, separan las anotaciones laterales y continúan entre páginas. La regresión del artículo a dos columnas detectó una confusión entre una tabla de abreviaturas y la bibliografía; una prueba directa exige mantenerlas separadas. La cobertura global bajó una décima (palabras 98,68 % → 98,65 %): la mejora demostrada es estructural, no de palabras.

**Perfil 1.13.0: cabeceras variables.** Las cabeceras corridas de capítulo no alcanzaban el 30 % de repetición y se confundían con el título real. Se separan por su alineación con un folio confirmado y un puesto tipográfico recurrente en al menos tres páginas, se registran una vez con todas sus páginas, y el título del cuerpo se reúne con su número lateral si reproduce el marcador completo. La referencia independiente (PDFium, franja de 43 puntos desde el borde superior en las páginas 25–345) fija 303 cabeceras y 152 títulos numerados.

| Propiedad | 1.11.0 | 1.12.0 | 1.13.0 |
| --- | ---: | ---: | ---: |
| Palabras conservadas (medidor corregido) | 98,68 % | 98,65 % | 98,29 % global; 98,65 % sin cabeceras repetidas |
| Texto ajustado | 99,55 % | 99,53 % | 99,56 % |
| Marcadores reconocidos | 179/214 | 199/214 | 201/214 |
| Conceptos / páginas / imágenes | 21 / 346 / 228 | 21 / 346 / 228 | 21 / 346 / 228 |
| Tabla de la página 85 (7 filas × 3 columnas) | mezcladas | 21/21 celdas | 21/21 celdas |
| Tabla de la página 251 | fragmentada | 21/21 celdas | 21/21 celdas |
| Listado de las páginas 109–110 | 10 bloques | 1 bloque, texto y sangría iguales a PDFium | igual |
| Cabeceras completas con sus páginas | — | 0/303 | 303/303 |
| Bloques de cuerpo en la franja de cabecera | — | 549 | 0 |
| Títulos con número lateral en el cuerpo | — | 93/152 | 152/152 |
| Conversión en el worker de 8080 / pico del hijo | 86,356 s / 199 MiB | 85,990 s / 202 MiB | 87,401 s / 202 MiB |

La caída de la cobertura global de 1.13.0 es por diseño: retira repeticiones de cabecera del cuerpo y las conserva en el inventario. En todas las tandas el bundle fue `complete`, de un intento, aceptado por `validate` sin red y con 0 fallos de salud en 89 sondeos.

**Carga y recursos (macOS, Apple M2).** Con un worker, una conversión tarda unos 82 s y dos simultáneas, 162 s (0,73 bundles/min); con dos workers, 83 s (1,44 bundles/min), sin fallos de salud y con p95 de `/health` inferior a 4 ms. Cada conversión usa aproximadamente un núcleo y su hijo llega a 293–351 MiB, por debajo de los límites de 500 MiB y 300 s; dos workers requieren reservar memoria para ambos hijos y el resto del servicio, y no es una recomendación universal de concurrencia. Los cinco bundles de la tanda son idénticos tras excluir la fecha y la duración del informe. Una imagen Linux de prueba, sin red, con un núcleo, cgroup de 600 MiB y los límites sin aumentar, convierte el PDF completo en 86,95 s con 202 MiB de RSS máxima; la prueba de humo de los cuatro formatos y del escaneo con OCR pasó.

**Límites que la primera evaluación dejó abiertos y siguen siendo ciertos:** una tabla con significado tabular que sobrevive solo como palabras no demuestra su estructura; el texto monoespaciado con anotaciones no debe tratarse como código ejecutable reconstruido; las cabeceras sin folio o sin posición recurrente no se detectan; el texto solo dentro de capturas de páginas mixtas no se transcribe ni lo evalúa PDFium; la contraportada (letra capitular y tipografías decorativas) es la peor página (91,74 % de texto ajustado); y 13 marcadores siguen sin reconocerse, sin rebajar el umbral del perfil.

## Parámetros y criterio operativo

Para este documento se conservan los límites por defecto: `OKF_MAX_MEMORY_MB=500`, `OKF_CONVERSION_TIMEOUT_S=300`, `OKF_MAX_PAGES=2000`, `OKF_MAX_IMAGE_PIXELS=33554432`, `OKF_MAX_OUTPUT_MB=512` y `OKF_OCR_ENABLED=false`. No hay páginas sin texto que necesiten OCR; aumentar sus páginas o su memoria no corrige los problemas de maquetación encontrados. Tampoco se suben memoria o plazo para esconder un fallo de validación.

La instancia de pruebas usa `RUST_LOG=okf_microservice=info`, que conserva los registros por intento sin el diagnóstico detallado de todas las fuentes de `pdf_oxide`. La captura acotada protege al supervisor incluso si se habilitan registros más verbosos. Los tiempos locales se miden en macOS arm64 sobre Apple M2 (8 núcleos, 16 GiB de RAM), con PostgreSQL local en Docker; no equivalen a capacidad garantizada del contenedor de producción. Las cifras de salud corresponden solo a esta carga de cinco trabajos, no a una prueba de saturación sostenida.

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

## Rendimiento de la conversión PDF (perfil 1.16.0)

**10 de noviembre de 2026.** Se perfiló la conversión de los PDF lentos con `sample` de macOS sobre un binario `--release` con símbolos, en un Apple M2 de 8 núcleos, con el subcomando `convert`, los límites por defecto y el OCR local desactivado. El código propio de maquetación (`layout`, enlaces, encabezados y pies) no llega al 2 % del tiempo. La conversión usa un solo núcleo y el tiempo se reparte entre `pdf_oxide` y el empaquetado:

| Original | Reparto antes del cambio |
| --- | --- |
| 575 páginas con OCR previo | Vistas 56 % (escalado de la imagen escaneada 44 %, texto invisible dibujado con color transparente 7 %, JBIG2 6 %); texto 27 %; reaperturas del documento 8 %; Deflate de las vistas y del original 5 %. |
| 346 páginas | Texto 50 %; imágenes JPEG CMYK convertidas a PNG 36 %. |
| Presentación de 53 páginas y 66 MB | Deflate del original al escribir el ZIP 42 %; descompresión, huella y relectura del original al validar 24 %. |
| Cuaderno apaisado de 65 páginas | Vistas de todas las páginas 72 %. |

Se aplicaron cuatro cambios que no alteran el texto extraído:

1. **ZIP:** una entrada de más de 256 KiB se guarda sin comprimir si Deflate no reduce al menos un 5 % una muestra de 128 tramos de 2 KiB. La muestra de 16 tramos de 16 KiB se descartó: en la presentación estimaba 0,88 frente al 0,97 real. El validador reserva la memoria del original sin comprimir antes de leerlo; sin esa reserva, el pico de la presentación subía de 329 a 390 MiB.
2. **Imágenes CMYK:** se convierten a RGB como antes y se adjuntan como JPEG de calidad 90, no como PNG. En las 196 imágenes afectadas del PDF de 346 páginas, el error absoluto medio frente al PNG anterior es 0,78 sobre 255 (máximo 2,95) y ocupan 24,1 MB en lugar de 63,8 MB.
3. **Vistas:** se renderizan en crudo y se codifican una vez, en gris o RGB si la página no usa más canales; la recuperación de listados OCR usa esos píxeles en lugar de decodificar el PNG. Se midieron también la compresión PNG por defecto (99 s y 476 MiB en el PDF de 575 páginas) y el filtro `Sub` (94 s y 166 MB de ZIP); se mantiene la compresión rápida con filtro adaptativo.
4. **Reaperturas:** por encima de 16 MiB, la codificación de imágenes y las vistas reutilizan el documento durante ocho páginas, como la lectura de texto, en lugar de reabrirlo en cada página.

Comparación en la misma sesión, versión anterior frente a la nueva:

| Original | Tiempo, s | CPU de usuario, s | Pico RSS, MiB | ZIP, MiB |
| --- | ---: | ---: | ---: | ---: |
| Presentación de 53 páginas | 3,87 → **2,15** | 3,14 → 1,47 | 329 → 330 | 75,0 → 77,1 |
| Cuaderno de 65 páginas | 2,72 → 2,93 | 2,60 → 2,82 | 178 → 178 | 12,7 → 12,3 |
| 346 páginas | 20,54 → **18,42** | 20,44 → 18,37 | 357 → 355 | 86,7 → **45,1** |
| 575 páginas con OCR previo | 94,31 → **85,99** | 93,29 → 85,40 | 426 → 427 | 202,6 → **141,3** |

Las tres conversiones de la presentación con la versión nueva tardaron entre 1,57 y 2,41 s. El cuaderno queda unos 0,2 s de CPU por encima: sus vistas en color pasan ahora por la conversión a RGB y el filtro adaptativo, y no se recuperó esa diferencia. Los PDF de menos de 3 MB no cambian de forma apreciable.

El Markdown de siete PDF reales es idéntico al anterior al normalizar la fecha y la extensión de las imágenes CMYK, incluidos los 780 bloques de código recuperados de las vistas en el PDF de 575 páginas. Las muestras y el corpus PDF públicos conservan sus cifras de cobertura y los bundles de referencia de los cuatro formatos no cambian, porque las vistas solo se enumeran y ningún ejemplo tiene imágenes CMYK.

**Pendiente.** Lo que queda del tiempo está en `pdf_oxide` y en el uso de un solo núcleo: el renderizado de las vistas, que escala la imagen del escaneo con filtro bilineal y dibuja también el texto invisible; la extracción de texto, que descomprime y analiza el contenido de cada página varias veces (fragmentos, caracteres, capa OCR, imágenes y vista); y el procesamiento en serie de páginas independientes. Paralelizar exige revisar antes la memoria de cada documento abierto, que se copia dos veces al abrirlo, y el límite de CPU del proceso (`RLIMIT_CPU` igual al plazo de conversión), que suma todos los hilos.

## Calidad del Markdown PDF (perfil 1.17.0)

**10 de noviembre de 2026.** A petición del usuario, que fija la completitud y la exactitud como requisitos máximos, se evaluó el Markdown de siete PDF privados. Por cada página se compararon las palabras de pdfium con las de los bloques del bundle localizados en ella, y se revisó el ruido que rodea al texto.

**Diagnóstico.** Frente al texto que extrae pdfium, el texto digital llegaba casi completo: entre el 0 % y el 0,8 % de palabras ausentes por documento, casi todas fragmentos que pdfium deja al partir palabras con guion o letras que separa en una presentación. Los problemas reales eran otros:

- **Ruido:** el frontmatter ocupaba entre el 49 % y el 68 % del Markdown en los PDF largos, por el localizador y la caja de cada bloque. En el PDF de 575 páginas con OCR previo, cada página insertaba una vista y una nota generada en mitad del texto y partía 184 párrafos.
- **Escaneos sin texto:** con el OCR desactivado por defecto, un escaneo sin capa de texto no aportaba ninguna palabra.
- **Exactitud:** pdf_oxide alinea con la línea base de un glifo grande (comillas de cita, capitulares) los fragmentos menores que lo rodean. La maquetación fundía entonces dos líneas e intercalaba sus letras. Los rótulos de diagramas y las celdas de varias líneas salían como filas de tabla con las frases partidas.

**Cambios del perfil 1.17.0.** Los localizadores de bloque pasan a `contenido/<concepto>.spans.json`. Una nota por concepto declara su texto OCR, y la vista de una página con texto OCR ya no parte el párrafo que continúa en ella. Los elementos de lista se unen con su continuación en la página siguiente. Los fragmentos alineados por error con un glifo grande recuperan la línea base de `extract_chars`, y dos tramos de líneas base distintas que se solapan en horizontal ya no forman una línea. Las filas que continúan los rótulos o celdas de la fila anterior se unen a ella. El OCR está activo por defecto.

| Original | Markdown, KB | Frontmatter | Notas generadas | Párrafos partidos |
| --- | ---: | ---: | ---: | ---: |
| Presentación de 53 páginas | 105 → 36 | 68 % → 6 % | 4 → 4 | 0 → 0 |
| Artículo de 9 páginas | 61 → 48 | 32 % → 14 % | 0 → 0 | 0 → 0 |
| PDF de 346 páginas | 1 706 → 809 | 54 % → 2 % | 1 → 1 | 0 → 0 |
| Cuaderno de 65 páginas | 280 → 145 | 66 % → 34 % | 67 → 67 | 0 → 0 |
| 575 páginas con OCR previo | 1 936 → 934 | 49 % → 3 % | 612 → 38 | 184 → 6 |
| Escaneo de una página | sin texto → texto con OCR (confianza media 98 %) | | | |

En el cuaderno se mantienen las 65 notas de sus vistas de diapositiva, que acompañan a diagramas. La completitud por página frente a pdfium no empeora en ningún documento (el PDF de 346 páginas pasa de 409 a 394 palabras ausentes). Las citas de la contraportada que salían con letras intercaladas se leen ahora enteras, y las tablas con frases partidas entre filas bajan de 12 a 6 en ese PDF y de 4 a 3 en la presentación. Las que quedan son maquetaciones complejas: un índice a dos columnas leído como tabla, capturas con texto dentro de una figura y diapositivas con cajas. Los errores de reconocimiento de una capa OCR previa o del OCR local (palabras unidas, signos) no se corrigen por inferencia.

**Hilos.** `OKF_CONVERSION_THREADS` reparte por tramos de ocho páginas la lectura del texto, la codificación de imágenes y las vistas. Cada tramo abre su propio documento, porque compartir uno entre hilos hacía que pdf_oxide decodificara mal imágenes y texto (dos bundles salieron `partial` y con texto distinto en la primera prueba). Con documentos propios, los bundles con uno y con cuatro hilos son idénticos en todas sus entradas, vistas incluidas. `OKF_MAX_CPU_S` fija el `RLIMIT_CPU`; sin declarar, vale el plazo por los hilos.

| Original | Antes | 1 hilo | 4 hilos | Pico RSS antes / 1 / 4 hilos, MiB |
| --- | ---: | ---: | ---: | ---: |
| Presentación de 53 páginas | 3,21 s | 1,55 s | 1,18 s | 327 / 333 / 739 |
| Cuaderno de 65 páginas | 2,67 s | 2,77 s | 1,62 s | 178 / 184 / 304 |
| PDF de 346 páginas | 20,26 s | 18,07 s | 8,01 s | 357 / 351 / 598 |
| 575 páginas con OCR previo | 92,88 s | 83,41 s | 27,17 s | 472 / 476 / 1 002 |

Cada hilo suma la memoria de su copia del documento y de sus páginas, así que cuatro hilos con estos originales necesitan subir `OKF_MAX_MEMORY_MB` (la medición usó 1 500). El pico del PDF de 575 páginas con un hilo varía unos 50 MiB entre ejecuciones del mismo binario (426–484 MiB) y ya rozaba el límite de 500 MiB antes de estos cambios. El valor por defecto sigue siendo un hilo.

Las muestras y el corpus PDF públicos conservan exactamente sus cifras de cobertura. Las referencias golden cambian por el perfil, por el traslado de los localizadores a sus archivos y por dos mejoras de texto en el corpus: celdas que recuperan su texto entero y una referencia bibliográfica unida a su continuación. Pruebas: `cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings` y `OKF_REQUIRE_OCR_MODELS=1 cargo test --locked`, con 134 pruebas de la biblioteca y las de integración sin fallos. No se ejecutó `tools/smoke.sh` con la imagen Docker.

## Artículos de arXiv (perfil 1.25.0)

**Fecha:** 27 de noviembre de 2026. **Originales:** cuatro preprints públicos de arXiv con plantillas de congreso distintas. No se versionan; se describen por su forma. Se convirtieron con el subcomando `convert` y se compararon página a página con sus vistas.

| Original | Forma | Antes (1.24.0) | Después (1.25.0) |
| --- | --- | --- | --- |
| 2610.20634v1 | 31 páginas a una columna (clase `article`), 42 marcadores | 19 conceptos y 42 encabezados; el número de cada sección quedaba como párrafo suelto («**1**») delante de su título | 19 conceptos y 43 encabezados, con su número («1 Introduction»); los apéndices «A Per-dataset results»… siguen casando con sus marcadores |
| 2610.34816v1 | 10 páginas a dos columnas (IEEE), 15 marcadores | un solo concepto y un encabezado: los títulos en versalitas («I. INTRODUCTION») salían como elementos de lista y no casaban con los marcadores sin número | 8 conceptos y los 15 encabezados de los marcadores, con los subapartados «A.»–«E.» |
| 2610.35868v1 | 21 páginas a dos columnas (ICML), sin marcadores | 19 conceptos y 19 encabezados inferidos por tamaño; ningún subapartado | 19 conceptos y 54 encabezados: 35 subapartados («3.1.») y los apéndices A–G con los suyos («A.1.») |
| 2610.36915v1 | 15 páginas a dos columnas (ACL), sin marcadores | un solo concepto: los títulos tienen 12 pt sobre texto de 11 pt (menos del 10 % mayor) y su número quedaba separado | 14 conceptos y 19 encabezados: secciones 1–8, «6.1»–«6.5», «Abstract», «Ethics and Broader Impact», «References» y los apéndices A y B |

**Defectos corregidos.**

- **Encabezados numerados del tamaño del texto.** La inferencia solo miraba el tamaño de letra. Ahora, sin marcadores, también son encabezados los párrafos breves con un número de sección y formato propio (negrita, cursiva o mayúsculas), si al menos tres de primer nivel siguen la serie desde el principio con texto entre ellos; sus subapartados, los apéndices por letra y los títulos sin número de igual formato («Abstract», «References») completan la estructura. La serie y el texto intermedio evitan tomar por secciones una lista numerada en negrita, y antes de la primera sección solo vale el último candidato, no los nombres de los autores en negrita.
- **Número separado del título.** ACL y la clase `article` separan el número del título con un espacio de más de un tamaño de letra; la maquetación los dejaba en líneas distintas. Ahora forman una línea si los dos van en negrita y con el mismo tamaño, y un marcador sin número casa con el párrafo numerado de su página.
- **Letras desplazadas por microtype.** Con expansión de fuentes, pdf_oxide colocaba la primera letra de algunos `TJ` encima de la siguiente («pAaired», «pArobability», «cAascade» en una lista). Esos fragmentos se leen ahora de `extract_chars`, que los coloca bien.
- **Diacríticos dibujados aparte.** TeX con fuentes Type 1 dibuja «ü» como «u» más «¨»; salían «Ja¨rvelin», «Ru¨ckle´» o un «**¨**» suelto encima del nombre de un autor. Se componen con la letra que tienen debajo: quedan dos de dieciocho, que pdf_oxide sitúa lejos de su letra.
- **Cursiva perdida.** pdf_oxide no marca como cursiva `NimbusRomNo9L-ReguItal`, la Times de estos artículos: dos de ellos no tenían ninguna cursiva y los subapartados en cursiva de IEEE no tenían formato propio. Se deduce también del nombre de la fuente.
- **Énfasis espurio en fórmulas.** El punto decimal en cursiva matemática partía las cifras («−0*.*56», 82 veces en un artículo). Un tramo sin letras ni cifras pierde el énfasis que no comparte con sus vecinos.
- **Párrafos partidos.** Una frase en negrita que ocupa una línea entera partía el párrafo en tres; y la hoja de la columna izquierda incluye el título a todo el ancho, de modo que una línea llena de la columna parecía corta y cerraba el párrafo tras un punto. Ahora el cambio de negrita no corta si la línea anterior llega al margen sin cerrar la frase y la nueva empieza en minúscula, y en texto justificado la línea que llega al borde de la columna no es corta. Un título numerado breve ya no absorbe la línea siguiente en negrita de otro tamaño.

**Mediciones.** Cobertura de palabras frente a pdfium, con el binario de `main` y con el del cambio en las mismas condiciones:

| Original | Palabras, antes → después | Orden interno | Contigüidad |
| --- | --- | --- | --- |
| 2610.20634v1 | 99,78 % → 99,79 % | 99,78 → 99,80 | 99,02 → 99,09 |
| 2610.34816v1 | 99,60 % → 99,59 % | 99,78 → 99,76 | 98,50 → 98,45 |
| 2610.35868v1 | 96,07 % → 96,23 % | 98,85 → 98,76 | 93,19 → 93,10 |
| 2610.36915v1 | 96,43 % → 96,43 % | 95,08 → 95,08 | 93,13 → 93,12 |

Las bajadas de una décima se concentran en las páginas de autores y bibliografía: pdfium deja los diacríticos como palabras sueltas («¨», «al¨»), y la palabra compuesta del bundle («Jünger») ya no coincide con la de la referencia. Las 13 muestras y originales reducidos del repositorio conservan exactamente su cobertura, y las 10 muestras con referencia exacta, su exactitud. Con el OCR activo y sus referencias revisadas, seis de los siete originales privados conservan exactamente su exactitud; el PDF de 346 páginas pasa de 99,97 % a 100 % de completitud y su orden baja de 88,93 a 88,90. En las referencias golden solo cambian tres cursivas recuperadas en el artículo del corpus y el recuento de encabezados de `informe_tecnico` (6 de sus 7 se infieren ahora por su numeración; los encabezados no cambian).

**Pendiente.**

- La página 14 de 2610.36915v1 tiene en las dos columnas recuadros de listados; el corte por columnas no la separa y sus líneas se intercalan (ya ocurría con el 1.24.0).
- Las tablas grandes con encabezados de dos niveles (la tabla I de 2610.34816v1) se reconstruyen solo en parte; el resto de sus celdas sale como líneas sueltas.
- En la bibliografía IEEE, una referencia cuya línea termina en dos puntos se parte por la sangría francesa de la siguiente, y otras se unen con la vecina.
- Los rótulos de un diagrama vectorial se escriben como párrafos sueltos, en el orden de su caja.
- Una palabra compuesta partida al final de línea pierde el guion si no aparece entera en otra línea («decision-based» → «decisionbased»), como prevé la especificación.

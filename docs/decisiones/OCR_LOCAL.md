# OCR local de PDF escaneados

**Fecha:** 6 de noviembre de 2026. Desarrolla el [stack, §4](STACK_TECNOLOGICO_MICROSERVICIO_OKF.md#4-renderizado-y-ocr-excepciones-explícitas). Las reglas del bundle están en la [especificación, §5](../especificacion/ESPECIFICACION_OKF.md) (perfil 1.1.0) y las variables, en la [arquitectura técnica, §5](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf).

## 1. Requisitos

- **Configurable desde `.env`:** `OKF_OCR_ENABLED`. Estuvo desactivado por defecto hasta el perfil 1.16.0; desde el 1.17.0 está activo si no se declara, porque la completitud del texto es el primer requisito de calidad. Apagado, los escaneos salen sin texto, con su vista y una nota. Las pruebas, los bundles de referencia y el fuzzing convierten sin OCR (`Config::default()`).
- **Transparente para quien sube el archivo:** la API no cambia. La clasificación de páginas que ya existía decide qué páginas necesitan OCR: una página sin texto propio con una imagen que cubre al menos el 30 % de ella. El OCR de página no se aplica a páginas mixtas, solo visuales ni a escaneos que ya traen una capa de OCR. Desde el perfil 1.18.0 se transcriben además las imágenes de contenido de cualquier página (§5).
- **Liviano:** sin bibliotecas del sistema ni servicios externos, dentro del mismo proceso conversor y de sus restricciones (rlimits, Landlock y seccomp).
- **Presupuesto propio** (stack, §4): `OKF_OCR_MAX_PAGES` páginas (40) y `OKF_OCR_MAX_IMAGES` imágenes (300) por documento, dos tercios del plazo de conversión en tiempo o en CPU y `OKF_OCR_MEMORY_MB` (512 desde el perfil 1.18.0; antes, 256) adicionales al límite de memoria de un PDF. Cuando se agota, las páginas restantes quedan como vista y el bundle sale `partial`, en vez de fallar por el límite de CPU del hijo.
- **Sin ocultar la incertidumbre:** la vista de la página se adjunta, una nota da la confianza junto al texto y los conceptos con texto de OCR son `draft`.

## 2. Motor elegido

`pdf_oxide`, que ya era el lector PDF, trae un OCR propio (función `ocr-tract`). Ejecuta PaddleOCR (detección DBNet y reconocimiento SVTR) con [tract](https://github.com/sonos/tract), en Rust. Se usan sus piezas de bajo nivel (`TextDetector`, `TextRecognizer`, `crop_text_region`); el resto está en `src/pdf/ocr.rs`.

Modelos, de [monkt/paddleocr-onnx](https://huggingface.co/monkt/paddleocr-onnx) en la revisión `7b02d0a`, con licencia Apache-2.0:

| Archivo | Modelo | Tamaño | SHA-256 |
| --- | --- | ---: | --- |
| `det.onnx` | Detección PP-OCRv3 móvil | 2,4 MB | `ee40e800…b935` |
| `rec.onnx` | Reconocimiento latino PP-OCRv5 móvil (32 idiomas, español incluido) | 7,9 MB | `614ffc2d…4409` |
| `dict.txt` | Diccionario de 502 caracteres (tildes, ñ, ¿ y ¡) | 1,6 KB | `3c0a8a79…13ff` |

`tools/ocr_models.sh` y el `Dockerfile` fijan la URL y la huella completas. La imagen copia también la ficha del repositorio (origen y licencia) y el texto de la licencia Apache-2.0 junto a los modelos.

**Alternativas descartadas:**

- **pdf_oxide con ONNX Runtime** (función `ocr`): más rápido, pero carga una biblioteca C++ nativa que habría que llevar a la imagen mínima y abrir dentro del hijo aislado, con efecto sobre Landlock y `RLIMIT_AS`.
- **Tesseract:** muy maduro para texto impreso en español, pero exige instalar Tesseract, Leptonica y sus bibliotecas en la imagen mínima y ejecutar otro programa desde el hijo, que Landlock y seccomp restringen.
- **`ocrs`:** Rust puro y ligero, pero entrenado sobre todo en inglés.
- **Detección PP-OCRv5 móvil** (4,8 MB): la misma exactitud que la v3 en las pruebas, con el doble de tamaño. La detección v5 de servidor ocupa 88 MB.

## 3. Ajustes medidos

La prueba de concepto se hizo fuera del repositorio sobre 30 páginas de cinco PDF del proyecto (los tres del corpus reducido y dos muestras), convertidas en imagen de dos formas:

- **limpia:** 200 DPI;
- **degradada:** 150 DPI, giro de 0,8°, ruido, desenfoque y JPEG de calidad 60.

El texto reconocido se comparó con el de pdfium. Usar el OCR de `pdf_oxide` tal cual no bastaba; hicieron falta cuatro ajustes:

1. **Enderezar.** Con 0,8° de giro, la detección unía líneas vecinas: pasaba de 18 líneas a 4, y la exactitud de las páginas degradadas caía al 6–42 %. Ni el ruido, ni el JPEG, ni la resolución tenían ese efecto. La página se endereza antes de detectar: el ángulo (±5°, en pasos de 0,1°) es el que maximiza la varianza del perfil de filas de la tinta, medida en una copia reducida. Detecta exactamente −0,80°.
2. **Anchos fijos.** tract prepara un plan por cada forma de entrada, y `pdf_oxide` solo redondea el ancho de cada línea a múltiplos de 4 píxeles. La memoria crecía con cada línea nueva (760 MB en dos páginas). Cada línea se lleva a 48 píxeles de alto y se rellena de blanco hasta un múltiplo de 64. La prueba de concepto usaba múltiplos de 320, pero el primer escaneo real (§4) mostró que tanto relleno hace perder espacios entre palabras en las líneas cortas: el título salía con todas sus palabras unidas. Con 64 px desaparece el defecto y la exactitud sube también en los escaneos sintéticos, sin más tiempo; la memoria crece unos 150 MB porque hay más formas.
3. **Tamaño de la página e hilos.** La página se renderiza a 200 DPI con el lado mayor limitado a 2400 píxeles (las diapositivas salían de 4000), y las líneas se reconocen en 2 hilos. Ampliar la imagen de detección más allá de 960 píxeles no mejoró el resultado y llevó la memoria a 1,3 GB.
4. **Tamaño de letra por la tinta.** La altura de la caja detectada no sirve para estimar el tamaño de letra: en la misma letra varía alrededor del 20 % (cuerpo en dos grupos de 10,3 y 12,3 pt), y un subtítulo era indistinguible del cuerpo. Se mide en el recorte a resolución completa, desde lo alto de las ascendentes hasta la línea base (0,73 cuerpos de letra). Así el cuerpo queda en 9,9–10,9 pt, los subtítulos en 11,8 y los títulos en 12,8–15,6. Los tamaños entre un 25 % por debajo y un 6 % por encima de la mediana de la página toman la mediana, para no partir párrafos: la medición falla sobre todo por defecto (líneas cortas, letra monoespaciada). Además, cada recorte se ensancha a la derecha 0,15 alturas de línea, porque la caja detectada dejaba fuera el punto final. Un margen mayor, o también a la izquierda, hacía leer el grano del papel como guiones.

El texto reconocido se convierte en caracteres posicionados (`Glyph`) repartidos sobre la caja de cada línea. A partir de ahí sigue la maquetación del texto digital, incluidos los encabezados y pies repetidos, porque el OCR se ejecuta antes de buscarlos.

## 4. Resultados

Prueba de concepto, macOS arm64, 2 hilos y anchos de 320 px (palabras de la referencia presentes en el OCR):

| Documento | Limpio | Degradado | Tildes y ñ |
| --- | ---: | ---: | ---: |
| `p4` (exportación de Word, español) | 99,3 % | 99,5 % | 99–100 % |
| `informe_tecnico` (muestra, español) | 99,5 % | 99,8 % | 100 % |
| `articulo_columnas` (muestra, español) | 96,0 % | 96,2 % | 98 % |
| `p2` (artículo en inglés, dos columnas) | 93,0 % | 92,3 % | — |
| `p1` (diapositivas) | 96–98 % de los caracteres | | 80–87 % |

En `p1`, pdfium parte las palabras de algunas diapositivas en letras sueltas, así que la comparación se hace por caracteres; el OCR las lee enteras.

Con anchos de 64 px, las versiones degradadas suben: `p2` del 92,3 % al 97,3 %, `articulo_columnas` del 96,2 % al 98,7 % y `p4` del 99,5 % al 99,8 %.

Rendimiento en macOS: unos 2,2 s por página de texto con 2 hilos (1,7 s con 4) y unos 430 MB de memoria residente para el OCR solo.

En el contenedor Linux (Docker Desktop, aarch64, 8 CPU), con la imagen y sus restricciones (Landlock, seccomp y `RLIMIT_AS`), las conversiones completas con OCR dieron:

| Documento | Páginas | Tiempo | Por página | Pico de memoria |
| --- | ---: | ---: | ---: | ---: |
| Escaneo real de una página (§4) | 1 | 3 s | 3 s | 221 MB |
| `escaneado_ocr.pdf` | 2 | 9 s | 4,5 s | 423 MB |
| `p4` degradado | 8 | 28 s | 3,5 s | 439 MB |
| `p2` degradado (artículo denso a dos columnas) | 8 | 58 s | 7,3 s | 469 MB |
| `p1` degradado (diapositivas) | 8 | 10 s | 1,3 s | 349 MB |

Con los anchos de 320 px de la prueba de concepto, los picos eran de 256–314 MB y `p2` tardaba 68 s.

Con 2 hilos, una página densa consume unos 15 s de CPU, y el hijo tiene un límite de CPU igual al plazo de conversión (300 s). Sin un presupuesto de tiempo, un documento de 20 a 40 páginas densas fallaría entero. Por eso el OCR no empieza una página más tras consumir dos tercios del plazo: con `OKF_CONVERSION_TIMEOUT_S=45`, `p2` degradado salió `partial` en 20 s, con 2 páginas transcritas y 6 como vista, sin fallar. Con el plazo por defecto se transcriben entre unas 12 páginas densas y 40 ligeras; para documentos escaneados largos hay que subir el plazo.

El pico de memoria en Linux llega a 469 MB, conversión incluida, y en macOS a 567 MB con `p2` degradado. Un PDF grande ya ocupaba 306 MB sin OCR. `OKF_OCR_MEMORY_MB=256` (756 MB en total) cubría estos casos; se comprobó con todos los documentos de la tabla en Linux. El texto de las imágenes lo subió a 512 (§5).

Integrado en el servicio, la muestra `examples/pdf/muestras/escaneado_ocr.pdf` (las dos páginas de `informe_tecnico.pdf`, degradadas por `create_scans.py`) sale `complete` y tiene:

- la confianza media del 97–98 %;
- los encabezados y pies repetidos («Informe técnico — Página N de 2») y los números de página detectados;
- tres secciones por encabezados inferidos y las listas numeradas;
- en macOS, 8,6 s y 502 MB de pico; en Linux, 9 s y 423 MB.

La prueba `ocr_transcribes_scanned_pages_through_the_common_layout` exige que recupere al menos el 95 % de las palabras del documento digital.

**Primer escaneo real (7 de noviembre).** Es un documento privado de `datos_de_prueba/`, que no entra al repositorio: una página generada como imagen JPEG de 1493 × 1570 px, sin capa de texto, con un título y tres párrafos en español en letra monoespaciada, con tildes y paréntesis. Sin OCR sale `partial`; con OCR, `complete`, con confianza media del 98 %, el título y los párrafos bien separados y todas las tildes correctas. Comparado con el texto de la vista, transcrito a mano, tiene dos errores: falta un espacio entre dos palabras del título, y una sigla en mayúsculas sale con dos de sus letras en minúscula. Con anchos de 320 px faltaban además los espacios de una línea entera; ese hallazgo llevó a los anchos de 64 px. Probar márgenes derechos de 0, 0,08, 0,1 y 0,12 no dio una tendencia: el espacio del título aparece o falta según el margen. Se mantuvo 0,15, el único que no pierde puntos finales en la muestra sintética.

## 5. Texto de las imágenes de contenido (perfil 1.18.0)

El plan 4, Q1, extendió el OCR a las imágenes que se adjuntan en páginas con texto digital: capturas de pantalla, diagramas, tablas pegadas como imagen. Antes, ese texto no llegaba al Markdown y ninguna métrica lo veía. Las reglas están en la [especificación, §5](../especificacion/ESPECIFICACION_OKF.md).

**Qué imágenes.** Las adjuntas que no son decorativas ni de plantilla, de al menos 64 × 16 píxeles. Una imagen repetida se transcribe una vez. Las de una página escaneada no, porque el OCR de página ya las lee.

**Ajustes:**
1. **Lienzos fijos.** Como con los anchos de línea (§3), tract prepara un plan por cada forma de entrada, y la detección sobre imágenes de tamaños distintos hacía crecer la memoria con cada imagen. Cada imagen se coloca, sobre fondo blanco y con su transparencia compuesta sobre blanco, en un lienzo cuadrado de 320, 640 o 960 píxeles (el primero que la contiene; si es mayor, su lado). Tres formas acotan los planes.
2. **Orientación.** Algunos PDF guardan las imágenes volteadas y las enderezan con la matriz con que las dibujan; `pdf_oxide` no la entrega. Sin corregirla, el OCR del PDF de 346 páginas daba texto al revés con confianza mediana del 66 %; leyendo la matriz del flujo de contenido y reorientando los píxeles, sube al 97–98 %. La misma corrección se aplica a las imágenes adjuntas, que antes salían volteadas en el bundle.
3. **Filtro de ruido.** Se descartan las líneas de confianza inferior al 80 %, y una imagen con menos de tres caracteres alfanuméricos no añade texto: así las fotografías no meten ruido. Las líneas que se solapan en vertical forman una fila, leída de izquierda a derecha.

**Resultados.** La muestra `imagenes_con_texto.pdf` (dos capturas de las otras muestras y un degradado sin texto) pasa del 45,9 % al 100 % de completitud, orden, orden interno y contigüidad, y el degradado no añade texto ([exactitud](EXACTITUD_DEL_TEXTO.md)). Con los originales privados (macOS arm64, un hilo, OCR activo, el binario de `--release`):

| Original | Imágenes con texto | Confianza mediana | Tiempo antes → ahora | Pico de memoria |
| --- | ---: | ---: | ---: | ---: |
| Artículo de 9 páginas | 3 de 3 | 99 % | 0,6 s → 7,8 s | 660 MB |
| Exportación de Word, 13 páginas | 3 de 3 | 97 % | 0,1 s → 3,9 s | 418 MB |
| Presentación de 53 páginas | 9 de 24 | 99 % | 1,6 s → 19 s | 854 MB |
| Cuaderno de 65 páginas | 0 de 10 | — | 2,9 s → 7,5 s | 508 MB |
| PDF de 346 páginas | 116 de 228 | 98 % | 18 s → 165 s | 914 MB |

En el PDF de 346 páginas, 114 de los 116 bloques superan el 90 % de confianza, y 95 imágenes quedaron sin OCR al agotarse el presupuesto de CPU con un hilo; el informe las declara. Las demás no tienen texto legible (fotografías, iconos). El cuaderno no gana texto: sus diagramas son vectoriales y su texto ya es digital. Que el texto añadido no sea ruido se comprobará con las referencias revisadas del plan 4, Q2.

**Escaneos largos.** El OCR de las páginas escaneadas va ahora en grupos de tantas páginas como `OKF_CONVERSION_THREADS`, con un solo motor compartido. Las líneas de cada página se reconocen además en 2 hilos, así que con un hilo de conversión el OCR consume 1,5 veces su tiempo en CPU y el límite de CPU se agota antes que el plazo. Un escaneo de 200 páginas (la muestra escaneada repetida, con motas distintas en cada página para que no se tome como plantilla):

| Hilos | Plazo | Páginas con OCR | Tiempo | CPU | Pico de memoria |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 300 s | 37 de 200 | 135 s | 208 s | 870 MB |
| 4 | 300 s | 96 de 200 | 170 s | 815 s | 1 923 MB |
| 4 | 900 s | 200 de 200, `complete` | 364 s | 1 844 s | 1 969 MB |

Con 4 hilos, la memoria se estabiliza en unos 1 800 MB desde el primer minuto: es la detección de cuatro páginas a la vez, no una acumulación. Cada hilo cuesta más CPU por página (9,2 s frente a 5,6 s con uno) porque comparten núcleos con los hilos del reconocimiento. Para escaneos de cientos de páginas hace falta subir el plazo (`OKF_CONVERSION_TIMEOUT_S`, al menos 700 s para 200 páginas con 4 hilos) y la memoria (`OKF_MAX_MEMORY_MB=1500` con los 512 del OCR); con los valores por defecto el bundle sale `partial` con las primeras páginas transcritas.

## 6. Límites conocidos

- Se ha medido con escaneos sintéticos y con un solo escaneo real, limpio y de una página. Faltan escaneos con fotografías, baja resolución, papel manchado o varias columnas (plan 1, R6); darán peores resultados. El texto manuscrito no se reconoce.
- En letra monoespaciada, el hueco entre palabras apenas se distingue del hueco entre letras, y alguna línea puede perder un espacio. El OCR puede confundir mayúsculas y minúsculas en siglas.
- El OCR no distingue negrita ni cursiva. Un subtítulo apenas mayor que el cuerpo puede no reconocerse como encabezado, y los niveles de los encabezados inferidos pueden variar entre secciones del mismo nivel.
- Las viñetas pueden leerse como guiones («—», «–») y la letra monoespaciada no se reconoce como código.
- El texto de una imagen es un bloque de líneas: no se reconstruyen sus tablas ni su código, y en un diagrama el orden de los rótulos es el de sus filas.
- El texto dibujado como trazos vectoriales, sin capa de texto ni imagen, no se transcribe.
- Un pánico dentro del OCR termina la conversión como fallo permanente, igual que cualquier pánico del conversor.
- Una página con un número de página digital añadido sobre el escaneo (por ejemplo, un sello Bates) tiene texto propio, no se clasifica como escaneo y no se transcribe.

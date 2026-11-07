# OCR local de PDF escaneados

**Fecha:** 6 de noviembre de 2026. Desarrolla el [stack, §4](STACK_TECNOLOGICO_MICROSERVICIO_OKF.md#4-renderizado-y-ocr-excepciones-explícitas). Las reglas del bundle están en la [especificación, §5](../especificacion/ESPECIFICACION_OKF.md) (perfil 1.1.0) y las variables, en la [arquitectura técnica, §5](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf).

## 1. Requisitos

- **Activable desde `.env`:** `OKF_OCR_ENABLED`, desactivado por defecto. Apagado, los bundles son los de antes (salvo `profile_version`).
- **Transparente para quien sube el archivo:** la API no cambia. La clasificación de páginas que ya existía decide qué páginas necesitan OCR: una página sin texto propio con una imagen que cubre al menos el 30 % de ella. No se aplica a páginas mixtas, solo visuales ni a escaneos que ya traen una capa de OCR.
- **Liviano:** sin bibliotecas del sistema ni servicios externos, dentro del mismo proceso conversor y de sus restricciones (rlimits, Landlock y seccomp).
- **Presupuesto propio** (stack, §4): `OKF_OCR_MAX_PAGES` páginas por documento (40), dos tercios del plazo de conversión en tiempo o en CPU y `OKF_OCR_MEMORY_MB` (256) adicionales al límite de memoria de un PDF. Cuando se agota, las páginas restantes quedan como vista y el bundle sale `partial`, en vez de fallar por el límite de CPU del hijo.
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

- **pdf_oxide con ONNX Runtime** (función `ocr`): más rápido, pero carga una biblioteca C++ nativa que habría que llevar a la imagen Distroless y abrir dentro del hijo aislado, con efecto sobre Landlock y `RLIMIT_AS`.
- **Tesseract:** muy maduro para texto impreso en español, pero exige instalar Tesseract, Leptonica y sus bibliotecas en Distroless y ejecutar otro programa desde el hijo, que Landlock y seccomp restringen.
- **`ocrs`:** Rust puro y ligero, pero entrenado sobre todo en inglés.
- **Detección PP-OCRv5 móvil** (4,8 MB): la misma exactitud que la v3 en las pruebas, con el doble de tamaño. La detección v5 de servidor ocupa 88 MB.

## 3. Ajustes medidos

La prueba de concepto se hizo fuera del repositorio sobre 30 páginas de cinco PDF del proyecto (los tres del corpus reducido y dos muestras), convertidas en imagen de dos formas:

- **limpia:** 200 DPI;
- **degradada:** 150 DPI, giro de 0,8°, ruido, desenfoque y JPEG de calidad 60.

El texto reconocido se comparó con el de pdfium. Usar el OCR de `pdf_oxide` tal cual no bastaba; hicieron falta cuatro ajustes:

1. **Enderezar.** Con 0,8° de giro, la detección unía líneas vecinas: pasaba de 18 líneas a 4, y la exactitud de las páginas degradadas caía al 6–42 %. Ni el ruido, ni el JPEG, ni la resolución tenían ese efecto. La página se endereza antes de detectar: el ángulo (±5°, en pasos de 0,1°) es el que maximiza la varianza del perfil de filas de la tinta, medida en una copia reducida. Detecta exactamente −0,80°.
2. **Anchos fijos.** tract prepara un plan por cada forma de entrada, y `pdf_oxide` solo redondea el ancho de cada línea a múltiplos de 4 píxeles. La memoria crecía con cada línea nueva (760 MB en dos páginas). Cada línea se lleva a 48 píxeles de alto y se rellena de blanco hasta un múltiplo de 320.
3. **Tamaño de la página e hilos.** La página se renderiza a 200 DPI con el lado mayor limitado a 2400 píxeles (las diapositivas salían de 4000), y las líneas se reconocen en 2 hilos. Ampliar la imagen de detección más allá de 960 píxeles no mejoró el resultado y llevó la memoria a 1,3 GB.
4. **Tamaño de letra por la tinta.** La altura de la caja detectada no sirve para estimar el tamaño de letra: en la misma letra varía alrededor del 20 % (cuerpo en dos grupos de 10,3 y 12,3 pt), y un subtítulo era indistinguible del cuerpo. Se mide en el recorte a resolución completa, desde lo alto de las ascendentes hasta la línea base (0,73 cuerpos de letra). Así el cuerpo queda en 9,9–10,9 pt, los subtítulos en 11,8 y los títulos en 12,8–15,6. Los tamaños entre un 25 % por debajo y un 6 % por encima de la mediana de la página toman la mediana, para no partir párrafos: la medición falla sobre todo por defecto (líneas cortas, letra monoespaciada). Además, cada recorte se ensancha a la derecha 0,15 alturas de línea, porque la caja detectada dejaba fuera el punto final. Un margen mayor, o también a la izquierda, hacía leer el grano del papel como guiones.

El texto reconocido se convierte en caracteres posicionados (`Glyph`) repartidos sobre la caja de cada línea. A partir de ahí sigue la maquetación del texto digital, incluidos los encabezados y pies repetidos, porque el OCR se ejecuta antes de buscarlos.

## 4. Resultados

Prueba de concepto, macOS arm64, 2 hilos (palabras de la referencia presentes en el OCR):

| Documento | Limpio | Degradado | Tildes y ñ |
| --- | ---: | ---: | ---: |
| `p4` (exportación de Word, español) | 99,3 % | 99,5 % | 99–100 % |
| `informe_tecnico` (muestra, español) | 99,5 % | 99,8 % | 100 % |
| `articulo_columnas` (muestra, español) | 96,0 % | 96,2 % | 98 % |
| `p2` (artículo en inglés, dos columnas) | 93,0 % | 92,3 % | — |
| `p1` (diapositivas) | 96–98 % de los caracteres | | 80–87 % |

En `p1`, pdfium parte las palabras de algunas diapositivas en letras sueltas, así que la comparación se hace por caracteres; el OCR las lee enteras.

Rendimiento en macOS: unos 2,2 s por página de texto con 2 hilos (1,7 s con 4) y unos 430 MB de memoria residente para el OCR solo.

En el contenedor Linux (Docker Desktop, aarch64, 8 CPU), con la imagen y sus restricciones (Landlock, seccomp y `RLIMIT_AS`), las conversiones completas con OCR dieron:

| Documento | Páginas | Tiempo | Por página | Pico de memoria |
| --- | ---: | ---: | ---: | ---: |
| `escaneado_ocr.pdf` | 2 | 9,5 s | 4,7 s | 288 MB |
| `p4` degradado | 8 | 30 s | 3,7 s | 286 MB |
| `p2` degradado (artículo denso a dos columnas) | 8 | 68 s | 8,5 s | 314 MB |
| `p1` degradado (diapositivas) | 8 | 11 s | 1,4 s | 256 MB |

Con 2 hilos, una página densa consume unos 15 s de CPU, y el hijo tiene un límite de CPU igual al plazo de conversión (300 s). Sin un presupuesto de tiempo, un documento de 20 a 40 páginas densas fallaría entero. Por eso el OCR no empieza una página más tras consumir dos tercios del plazo: con `OKF_CONVERSION_TIMEOUT_S=45`, `p2` degradado salió `partial` en 20 s, con 2 páginas transcritas y 6 como vista, sin fallar. Con el plazo por defecto se transcriben entre unas 12 páginas densas y 40 ligeras; para documentos escaneados largos hay que subir el plazo.

El pico de memoria en Linux (hasta 314 MB, conversión incluida) cabe en los 500 MB por defecto. Pero en macOS el OCR solo llega a 430 MB, y un PDF grande ya ocupaba 306 MB sin OCR. `OKF_OCR_MEMORY_MB=256` cubre ambos casos (756 MB en total); se comprobó con `p2` degradado en Linux.

Integrado en el servicio, la muestra `examples/pdf/muestras/escaneado_ocr.pdf` (las dos páginas de `informe_tecnico.pdf`, degradadas por `create_scans.py`) sale `complete` y tiene:

- la confianza media del 97–98 %;
- los encabezados y pies repetidos («Informe técnico — Página N de 2») y los números de página detectados;
- tres secciones por encabezados inferidos y las listas numeradas;
- en macOS, 9,3 s y 400 MB de pico; en Linux, 9,5 s y 288 MB.

La prueba `ocr_transcribes_scanned_pages_through_the_common_layout` exige que recupere al menos el 95 % de las palabras del documento digital.

## 5. Límites conocidos

- Solo se ha medido con escaneos sintéticos, generados a partir de PDF digitales con letra nítida. Un escaneo real (fotografías, baja resolución, papel manchado o texto manuscrito) dará peores resultados. El corpus pendiente (plan 1, R6) incluye un PDF escaneado real.
- El OCR no distingue negrita ni cursiva. Un subtítulo apenas mayor que el cuerpo puede no reconocerse como encabezado, y los niveles de los encabezados inferidos pueden variar entre secciones del mismo nivel.
- Las viñetas pueden leerse como guiones («—», «–») y la letra monoespaciada no se reconoce como código.
- Un pánico dentro del OCR termina la conversión como fallo permanente, igual que cualquier pánico del conversor.
- Una página con un número de página digital añadido sobre el escaneo (por ejemplo, un sello Bates) tiene texto propio, no se clasifica como escaneo y no se transcribe.

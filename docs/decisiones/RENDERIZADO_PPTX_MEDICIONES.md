# Decisión de renderizado PPTX

**Decisión para la primera entrega:** usar `betteroffice-pptx` 0.5.1 con su backend `raster` de Rust como único renderizador de producción. LibreOffice y Poppler se usaron solo en la comparación local y no se incorporan al despliegue.

## Comparación local

Se procesó `examples/pptx/multiple.pptx` (3 diapositivas; la tercera está oculta e incluye una tabla y una imagen). La ruta BetterOffice parsea el PPTX y crea un PNG por diapositiva. La ruta alternativa ejecuta LibreOffice Impress 26.8.0.0 alpha con `ExportHiddenSlides=true`, exporta a PDF y convierte las páginas a PNG con Poppler a 96 dpi. Las mediciones son de una ejecución en macOS, sin compilación incluida; sirven para comparar este ejemplo, no como presupuesto universal.

| Ruta | Diapositivas / PNG | Tiempo de ruta | CPU usuario / sistema | RSS máximo del proceso | Tamaño PNG total |
| --- | ---: | ---: | ---: | ---: | ---: |
| BetterOffice Raster (Rust) | 3 / 3 | 1,071 s proceso completo (0,292 s parseo y renderizado interno) | 0,282 s / 0,024 s | 30,0 MB | 82.536 bytes |
| LibreOffice → PDF → Poppler PNG | 3 / 3 | 1,477 s | 0,425 s / 0,503 s | 106,8 MB | 39.792 bytes |

RSS es el máximo del proceso hijo medido por `getrusage`; para LibreOffice incluye la exportación y el rasterizado de páginas. BetterOffice no omitió imágenes en este fixture (`skipped_images=0`). Ambas rutas incluyeron la diapositiva oculta cuando se configuró LibreOffice para hacerlo.

## Revisión visual y alcance

Las dos rutas conservaron el título, el texto y la posición de la imagen. LibreOffice dibujó los bordes de las celdas; BetterOffice no. BetterOffice mantuvo filas, columnas, alineación y contenido de la tabla, que también aparece estructurado en el concepto de la diapositiva. Para esta entrega la pérdida de bordes es acotada y el bundle conserva el original, la tabla extraída y su vista; se acepta esa diferencia para evitar incorporar LibreOffice, Poppler y el paso intermedio de PDF al worker.

La prueba funcional usa ambos ejemplos PPTX. La lectura de valores almacenados de gráficos se prueba por separado con un fragmento OOXML; el ejemplo de comparación no incluye un gráfico completo. Los gráficos cuyos valores estén en partes OOXML relacionadas se incorporan como texto recuperado; la fidelidad visual de tipos de gráfico complejos queda pendiente de un corpus real.

La comparación es orientativa: los ejemplos son pequeños y sintéticos. Revisar esta decisión si presentaciones reales muestran pérdidas importantes en tablas, efectos, gráficos, imágenes SVG o fuentes no disponibles. La fuente TrueType usada para el renderizado se configura con `OKF_FONT_PATH` o se localiza en rutas estándar del sistema.

## Corpus real

Se renderizaron tres presentaciones del curso, cuyos originales se guardan localmente en `datos_de_prueba/` (no versionado), con `okf-microservice bench-render` en compilación *release* sobre macOS. Solo se midió BetterOffice; LibreOffice no se volvió a ejecutar.

| Presentación | Original | Diapositivas / PNG | Tiempo de proceso | CPU usuario / sistema | RSS máximo | Tamaño PNG total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `s1.pptx`: formas y texto sueltos, 103 SVG | 2,2 MB | 29 / 29 | 0,85 s | 0,80 s / 0,04 s | 313,8 MB | 8.270.572 bytes |
| `s2.pptx`: marcadores de posición, capturas PNG de ~1,5 MB | 22,8 MB | 27 / 27 | 1,24 s | 0,88 s / 0,04 s | 366,5 MB | 12.505.861 bytes |
| `s3.pptx`: como `s2`, con 5 diapositivas animadas | 18,2 MB | 34 / 34 | 1,02 s | 0,97 s / 0,03 s | 301,0 MB | 14.869.987 bytes |

En ningún caso se omitieron imágenes (`skipped_images=0`). La memoria es unas diez veces la del ejemplo sintético y debe considerarse al fijar el límite del worker. La compilación de depuración es mucho más lenta: la primera conversión de `s1` tardó 33 s; las mediciones de presupuesto deben hacerse en *release*.

**Revisión visual.** Los diagramas de `s1`, construidos con decenas de formas, SVG y flechas, conservan posición, colores y texto. Las presentaciones usan Barlow, Fira Sans y Lexend, que no estaban registradas; el renderizado sustituyó la fuente sin desbordes visibles, aunque con métricas distintas a las originales. Las diapositivas animadas de `s3` (21, 23, 25, 27 y 29) muestran una imagen a pantalla completa sobre el texto, porque la vista dibuja todas las formas a la vez: el conversor lo declara como `view_hides_text` y conserva el texto en el concepto. El corpus no contiene tablas nativas, gráficos ni SmartArt, de modo que esas fidelidades siguen sin medirse con material real.

Las pruebas de aceptación usan copias reducidas de esas presentaciones en `examples/pptx/corpus/`, generadas con `examples/pptx/reduce_corpus.py`: conservan todo el XML y reemplazan imágenes y fuentes incrustadas, de modo que sirven para la extracción, no para medir el renderizado.

**Conclusión:** se mantiene BetterOffice Raster como único renderizador. Revisar si aparecen tablas o gráficos reales con pérdidas, o si el límite de memoria del worker no admite unos 400 MB por conversión.

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

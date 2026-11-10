# Plan 4: completitud y exactitud del texto

**Fecha:** 10 de noviembre de 2026.  
**Punto de partida:** el perfil 1.17.0 y la [medición de exactitud](../decisiones/EXACTITUD_DEL_TEXTO.md), con siete PDF privados y las muestras públicas ([calidad del Markdown PDF](../decisiones/PRUEBAS_PDF_REALES.md#calidad-del-markdown-pdf-perfil-1170)).  
**Planes anteriores:** [Plan 1](PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md) (`R1`…`R9`), [Plan 2](PLAN_2_CALIDAD_Y_ROBUSTEZ.md) (`C1`…`C10`) y [Plan 3](PLAN_3_CONTRATO_DE_SALIDA_PARA_RAG.md) (`I1`…`I9`).

## 1. Objetivo

La completitud y la exactitud del texto son los requisitos máximos de calidad del Markdown. Con el perfil 1.17.0, el texto digital de un PDF llega casi completo (entre 99,2 % y 100 % de las palabras de pdfium) y en su orden dentro de cada bloque (orden interno entre 94 % y 100 %). Quedan tres huecos:

1. **Texto que no es texto en el PDF.** Lo que está dentro de imágenes no llega al Markdown, y los escaneos largos solo se transcriben en parte.
2. **Qué es «correcto» en un original real.** Las referencias exactas cubren cuatro maquetaciones sintéticas. En los originales reales solo hay una referencia aproximada (pdfium), y nada mide la estructura: encabezados, tablas, listas y código.
3. **Defectos de maquetación ya localizados** que la medición no deja pasar a cero.

Este plan tiene tres trabajos, uno por hueco. Tamaños relativos, como en los planes anteriores: **S** (un cambio acotado en un módulo), **M** (varios módulos o una herramienta nueva), **L** (cambio transversal).

## 2. Situación de partida

| Hueco | Evidencia (perfil 1.17.0) | Trabajo |
| --- | --- | --- |
| Texto dentro de imágenes | El OCR solo se aplica a páginas escaneadas sin texto. Se adjuntan sin transcribir 228 imágenes en el PDF de 346 páginas y 33 en la presentación de 53. En el cuaderno de 65 páginas, los diagramas de 65 vistas tampoco se transcriben. Ni pdfium ni la cobertura lo ven, así que no aparece en ninguna cifra. | `Q1` |
| Escaneos largos | `OKF_OCR_MAX_PAGES` (40) y dos tercios del plazo de 300 s limitan el OCR a unas 12 páginas densas o 40 ligeras por documento ([OCR local](../decisiones/OCR_LOCAL.md)). El OCR va en serie aunque la conversión admita varios hilos (`OKF_CONVERSION_THREADS`). | `Q1` |
| Referencias de originales reales | El orden global frente a pdfium no prueba un error: pdfium deja los rótulos de figuras al final de la página y, en exportaciones de Canva, separa letras. Solo el orden interno y la contigüidad son fiables con él. | `Q2` |
| Estructura | La exactitud mide palabras, no marcado: un encabezado perdido, una tabla con celdas desplazadas o un código sin sangría pasan si las palabras están en orden. | `Q2` |
| Defectos de maquetación | Tablas falsas (un índice a dos columnas, capturas con texto, cajas de diapositivas: 6 en el PDF de 346 páginas, 3 en la presentación). Letras capitales separadas («M ost»). Ligaduras partidas («Th e», «prof iles»). Texto girado al final de la página. Títulos inferidos repetidos: las tres secciones de la presentación tienen el mismo. | `Q3` |

## 3. Trabajos

### Q2. Referencias revisadas de originales reales y exactitud de la estructura (M)

Va primero porque fija la vara con la que se miden `Q1` y `Q3`.

**Problema.** Sin referencias exactas de originales reales, una mejora o una regresión en un PDF real solo se ve con métricas aproximadas o a mano; así se encontró la regresión del índice del libro (perfil 1.17.0). Y ninguna métrica cubre la estructura.

**Trabajo.**

1. **Páginas difíciles de cada original privado.** Seleccionar entre 10 y 20 por documento: tablas, índices, figuras con rótulos, varias columnas, código, capitulares, diapositivas y escaneos. `tools/exactitud.py` genera un borrador de su referencia con el texto del bundle en orden de lectura y una vista de cada página. Una persona lo corrige contra la vista. Las referencias se guardan junto a los originales en `datos_de_prueba/` y no se versionan. La documentación las describe solo por su forma y sus cifras.
2. **Estructura en la referencia.** El formato admite, además del texto de cada página, sus bloques con tipo: encabezado con nivel, párrafo, lista, tabla con filas y celdas, código. Las muestras públicas los obtienen de `create_samples.py`, como hoy el texto.
3. **Métricas de estructura** en `tools/exactitud.py` y en `src/exactness.rs`:
   - **encabezados:** texto y nivel;
   - **tablas:** celdas en su fila y columna, y tablas que sobran o faltan;
   - **listas:** elementos y su numeración;
   - **código:** líneas y sangría relativa.
4. **Mínimos por documento** para los originales privados, en un archivo local que acompaña a las referencias. Un cambio del adaptador PDF se mide con ellos antes de publicarse (`AGENTS.md`).

**Aceptación.**
- Al menos 60 páginas revisadas entre los originales privados, con sus cifras de texto y estructura.
- Las muestras públicas cubren cada tipo de bloque con mínimos en las pruebas.
- Reintroducir la regresión del índice (unir filas con números de página) hace fallar la medición.

**Coste y riesgo.** La revisión necesita tiempo de una persona, unas dos horas para 60 páginas con el borrador. Sin ella, `Q1` y `Q3` siguen dependiendo de pdfium.

### Q1. Texto dentro de imágenes y escaneos largos (L)

El mayor hueco de completitud.

**Problema.** El texto de diagramas, capturas de pantalla, diapositivas hechas como imagen y fotografías de documentos no llega al Markdown. Los escaneos de más de unas 40 páginas quedan sin texto en su mayor parte.

**Trabajo.**

1. **OCR de imágenes de contenido.** Se aplica a las imágenes adjuntas que no son decorativas ni de plantilla y que superan un tamaño mínimo, y a las vistas de páginas visuales y de exportaciones por diapositivas. El texto reconocido se escribe debajo de la imagen como bloque generado y no verificado, con su confianza (`<!-- okf-generated -->`). Una imagen sin texto reconocible o con confianza baja no añade nada, para no meter ruido de fotografías. El concepto lleva `status: draft` como con el OCR de escaneos.
2. **OCR en paralelo** con `OKF_CONVERSION_THREADS`, por tramos de páginas con su propio documento, como la lectura de texto. Una sola copia de los modelos por proceso si `tract` lo admite; si no, se mide la memoria por hilo.
3. **Presupuesto por imágenes y páginas.** `OKF_OCR_MAX_PAGES` pasa a contar unidades de OCR (páginas e imágenes), con un valor por defecto revisado tras medir. El informe declara qué imágenes y páginas quedaron sin OCR y por qué.
4. **Muestras con texto en imágenes** en `create_samples.py`: un diagrama con rótulos, una captura con texto y una fotografía sin texto. Sus referencias exactas incluyen el texto de cada imagen, de modo que la exactitud mide también este OCR.

**Aceptación.**
- **Muestras:** completitud de al menos el 95 % del texto de sus imágenes y ningún texto añadido para la fotografía.
- **Originales privados:** se informan las palabras añadidas por OCR de imágenes, y una muestra revisada de `Q2` comprueba que no sean ruido.
- **Escaneos largos:** uno de 200 páginas se transcribe completo con 4 hilos dentro de los límites, que se miden y documentan.
- **Perfil:** la especificación regula dónde y cómo se escribe el texto de una imagen. Sube el perfil.

**Coste y riesgo.** El OCR de imágenes multiplica el tiempo de conversión de documentos con muchas figuras: en el PDF de 346 páginas, unas 200 imágenes a 1,7–2,2 s cada una, divididas por los hilos. Hay que medir antes de fijar los valores por defecto. El OCR local no distingue negrita ni cursiva y une palabras («DEPRUEBA»). Ese defecto se aborda aparte, dentro de este trabajo, si la medición lo muestra frecuente.

### Q3. Maquetación residual guiada por la medición (M)

Defectos ya localizados, cada uno con su muestra y su mínimo.

**Problema.** Quedan defectos de exactitud concretos que la medición de `Q2` puede comprobar:
- tablas falsas en índices a dos columnas, capturas y cajas de diapositivas;
- letras capitales separadas de su palabra;
- ligaduras partidas por un espacio;
- texto girado escrito al final de la página en vez de en su posición;
- títulos inferidos que se repiten en todas las secciones de un documento.

**Trabajo.**

1. **Tablas.** Una región con dos columnas de texto corrido (líneas que continúan la frase de la anterior en la misma columna, como un índice o un glosario) se lee como columnas y no como tabla. Una captura o una figura con texto no se convierte en tabla Markdown.
2. **Capitales.** Una letra de una o dos posiciones mucho mayor, alineada con el borde superior de un párrafo, se une a la primera palabra de ese párrafo («Most»).
3. **Ligaduras.** Un espacio que pdf_oxide inserta tras una ligadura («Th», «fi», «fl», «ff») sin hueco real entre glifos se suprime, con la misma prueba geométrica que hoy separa palabras.
4. **Texto girado.** Se coloca en el orden de lectura según su posición, no al final de la página, salvo que sea marginal (sellos, rótulos de borde).
5. **Títulos repetidos.** Un título inferido que se repite en las secciones de un documento no divide ni da nombre a la sección. Se usa el siguiente encabezado propio o el primer párrafo, y el informe lo declara.

Cada corrección añade su caso a `create_samples.py` con referencia exacta y mínimos, y se mide con los originales privados de `Q2` antes de publicarse.

**Aceptación.**
- Cero tablas falsas en las páginas revisadas de `Q2`.
- Las capitales, ligaduras y textos girados de esas páginas, exactos.
- Las secciones de la presentación, con títulos distintos.
- Ninguna métrica de los originales privados empeora más de 0,1 puntos en ningún documento.
- Cada regla nueva o cambiada de la especificación sube el perfil.

**Coste y riesgo.** Las heurísticas de tablas y columnas afectan a muchos documentos. Por eso cada cambio se mide con todo el conjunto de `Q2` y no solo con su caso.

## 4. Versión del perfil y compatibilidad

`Q1` y `Q3` cambian reglas de la especificación: cada uno sube la versión menor del perfil con su registro de cambios, su esquema y los bundles de referencia revisados. `Q1` añade bloques generados, sin cambiar los campos existentes, y `Q3` cambia la lectura de casos que hoy son defectos. Las versiones anteriores pasan a `PREVIOUS_PROFILES`. `Q2` no cambia el contrato.

## 5. Orden

| Fase | Trabajos | Motivo |
| --- | --- | --- |
| A. Vara de medir | `Q2` | Sin referencias revisadas ni métricas de estructura, `Q1` y `Q3` se juzgan a ojo o con pdfium. |
| B. Completitud | `Q1` | El mayor hueco: texto que hoy no llega en ningún caso. Sus muestras se pueden preparar en paralelo con `Q2`. |
| C. Exactitud | `Q3` | Correcciones acotadas, cada una comprobada con el conjunto de `Q2`. |

**Antes de empezar:** la instancia local de `8080` y su PostgreSQL ya no existen en Docker Desktop. Hay que reconstruir la imagen con el perfil 1.17.0 y levantarlas de nuevo para medir con el servicio completo.

**Fuera de este plan:**
- **Versión del Markdown sin marcadores para el RAG:** una decisión de contrato pendiente del consumidor.
- **Caché de la imagen en la CI:** la descarga lenta de la caché de GitHub Actions es un asunto operativo de la CI.
- **Evaluación DOCX y corpus del plan 1 (R6):** siguen en el [estado del proyecto](../estado/ESTADO_DEL_PROYECTO.md#6-roadmap-pendiente).

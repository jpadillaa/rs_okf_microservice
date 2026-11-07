# Cobertura frente a una referencia independiente

**Fecha:** 1 de noviembre de 2026.  
**Origen:** [plan 1, R2](../planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md#r2-referencia-de-cobertura-independiente-m). Sustituye, como evidencia, la medición de Xberg que no se hizo en PPTX, DOCX y XLSX ([stack, §3](STACK_TECNOLOGICO_MICROSERVICIO_OKF.md#3-extracción-y-procedencia-por-formato), «Abandono de Xberg»).

## 1. Método

`tools/coverage.py` convierte cada documento con el ejecutable del servicio (`okf-microservice convert`, el mismo subcomando que usa el worker) y compara las palabras del bundle con las de una extracción hecha con herramientas que no comparten código con los lectores del servicio:

| Formato | Referencia | Partes leídas directamente del XML |
| --- | --- | --- |
| PDF | pdfium (`pypdfium2` 5.14.0), texto de cada página | — |
| DOCX | `python-docx` 1.2.0: párrafos, tablas (cada celda combinada una vez), encabezados y pies | notas al pie y finales, comentarios, cuadros de texto (sin la copia VML), ecuaciones, SmartArt, gráficos y texto alternativo de los dibujos |
| PPTX | `python-pptx` 1.0.2: formas (con grupos), tablas, notas y texto alternativo de las imágenes | gráficos, SmartArt y comentarios |
| XLSX | `openpyxl` 3.1.5: nombres de hoja, valores almacenados y notas | gráficos y comentarios encadenados |

Las versiones están fijadas en la cabecera del script, que se ejecuta con `uv run tools/coverage.py DOC…`. Si a un paquete le falta una parte que sus relaciones nombran (las copias reducidas del corpus y las muestras con pérdidas), el script quita esa relación o pone una imagen en blanco en su lugar, porque las tres bibliotecas rechazan esos paquetes y el servicio, como Office, los admite.

Las dos partes se reducen a palabras (NFKC, sin distinguir mayúsculas, secuencias de letras y dígitos) y se comparan como multiconjuntos, en los dos sentidos:

- **Palabras (%)**: proporción de las palabras de la referencia que están en el bundle.
- **Texto (%)**: igual, pero cuenta como presente una palabra que falta, de cuatro caracteres o más, si aparece dentro del texto del bundle sin espacios. Recoge el mismo texto agrupado de otra forma: pdfium une palabras separadas por posición y conserva los guiones de fin de línea.
- **Sobran**: palabras del bundle que no están en la referencia: rótulos que escribe el conversor, texto que la referencia no ve y, si lo hubiera, texto duplicado o inventado.

Del bundle se leen los conceptos Markdown y `documento.md` (sin comentarios HTML, destinos de enlaces ni frontmatter, salvo los campos con texto del original: `kicker`, `section` y `display_label`) y los recursos CSV; no los índices ni el informe.

Con `--write-reference`, el script guarda la lista de palabras de la referencia de cada documento de `examples/` en `examples/<formato>/cobertura/<grupo>/<nombre>.tsv` (palabra y número de apariciones), para que la CI compare la cobertura sin Python ([plan 2, C2](../planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md#c2-cobertura-como-control-de-la-ci-s)). Los documentos de fuera de `examples/` no tienen lista: pueden ser privados, como el recibo.

## 2. Corpus real

Las copias reducidas de `examples/*/corpus/` dan exactamente las mismas cifras que los originales de `datos_de_prueba/`, lo que confirma que la reducción conserva el texto. El recibo (`p3`), que no se versiona, se midió solo en local.

| Documento | Estado | Palabras de la referencia | Palabras (%) | Texto (%) | Faltan | Sobran |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `d1.docx` | complete | 707 | 100,0 | 100,0 | 0 | 61 |
| `d2.docx` | complete | 2449 | 100,0 | 100,0 | 0 | 148 |
| `d3.docx` | complete | 4013 | 100,0 | 100,0 | 0 | 164 |
| `p1.pdf` (presentación de Canva, 53 páginas) | complete | 2792 | 96,88 | 99,64 | 87 | 2277 |
| `p2.pdf` (artículo a dos columnas) | complete | 5848 | 95,06 | 98,48 | 289 | 349 |
| `p3.pdf` (recibo, solo en local) | complete | 148 | 100,0 | 100,0 | 0 | 32 |
| `p4.pdf` (exportación de Word) | complete | 3169 | 100,0 | 100,0 | 0 | 242 |
| `s1.pptx` | complete | 8144 | 97,11 | 97,11 | 235 | 839 |
| `s2.pptx` | partial | 505 | 100,0 | 100,0 | 0 | 652 |
| `s3.pptx` | partial | 1292 | 100,0 | 100,0 | 0 | 773 |

XLSX no tiene corpus real; se medirá cuando [R6](../planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md#r6-corpus-representativo-m-continuo) aporte un libro.

### 2.1 Qué falta y por qué

Todas las palabras que faltan están explicadas:

- **`p1.pdf`.** Canva coloca cada palabra por posición y pdfium las une (`bibianaalfonsorivera`, `sonmediosynofines`); en el bundle están separadas. Después de reagruparlas quedan 10 palabras: fragmentos de menos de cuatro letras de palabras partidas por pdfium (`t`, `ens`, `sie`, `v`) y números de página impresos, que van a `page_labels`.
- **`p2.pdf`.** El encabezado de página «Cheng et al. Advances in Simulation (2025) 10:22» y el pie «Page N of 9» se repiten en casi todas las páginas y el bundle los registra una vez en `documento.md`, como manda la especificación. El resto son fragmentos de palabras partidas con guion al final de línea (`gen`, `pro`, `ers`), que el bundle une.
- **`p4.pdf`.** No falta ninguna desde el perfil 1.2.0 (7 de noviembre). Antes faltaba una palabra: el guion de `langchain-ai.github.io` se suprimía al unir el final de línea (ver §4).
- **`s1.pptx`.** 208 palabras son el texto alternativo `preencoded.png` de 104 iconos: un nombre de archivo, que el conversor descarta junto con el icono decorativo. Las otras 27 son el total «/ 29» del número visible de cada diapositiva («01 / 29»), del que `display_label` conserva solo el número.

### 2.2 Qué sobra y por qué

Lo que sobra son sobre todo rótulos del conversor: «Página N» y «Vista de la página» en PDF; «Diapositiva», «Vista visual», «Notas del presentador», «Contenido solo visual» y la nota de título inferido en PPTX; y en DOCX los títulos de sección repetidos en `documento.md` y los marcadores de nota. En `p1.pdf`, además, las palabras que pdfium une se cuentan por separado en el bundle. Entre las 25 palabras que más sobran en cada documento no aparece texto duplicado ni inventado; el resto no se revisó una a una.

## 3. Muestras sintéticas

No son evidencia independiente: se generaron con las mismas bibliotecas que sirven de referencia y solo contienen lo que esas bibliotecas saben escribir. Se miden para tener sus listas de referencia ([C2](../planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md#c2-cobertura-como-control-de-la-ci-s)).

| Documento | Estado | Palabras de la referencia | Palabras (%) | Texto (%) | Faltan | Sobran |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `docx/breve.docx` | complete | 16 | 100,0 | 100,0 | 0 | 19 |
| `docx/estructura.docx` | complete | 324 | 99,07 | 99,07 | 3 | 129 |
| `docx/muestras/anexos_con_perdidas.docx` | partial | 269 | 99,26 | 99,63 | 2 | 70 |
| `docx/muestras/formato_directo.docx` | complete | 226 | 100,0 | 100,0 | 0 | 45 |
| `docx/muestras/guia_tecnica.docx` | complete | 336 | 100,0 | 100,0 | 0 | 70 |
| `docx/muestras/informe_revisado.docx` | complete | 410 | 100,0 | 100,0 | 0 | 340 |
| `pdf/muestras/articulo_columnas.pdf` | complete | 530 | 97,74 | 99,62 | 12 | 162 |
| `pdf/muestras/breve.pdf` | complete | 99 | 100,0 | 100,0 | 0 | 34 |
| `pdf/muestras/escaneado_mixto.pdf` | partial | 88 | 100,0 | 100,0 | 0 | 137 |
| `pdf/muestras/escaneado_ocr.pdf` | partial | 0 | — | — | 0 | 69 |
| `pdf/muestras/informe_tecnico.pdf` | complete | 614 | 99,35 | 99,84 | 4 | 183 |
| `pptx/multiple.pptx` | complete | 31 | 100,0 | 100,0 | 0 | 50 |
| `pptx/rich.pptx` | complete | 9 | 100,0 | 100,0 | 0 | 52 |
| `pptx/simple.pptx` | complete | 16 | 100,0 | 100,0 | 0 | 17 |
| `pptx/visual.pptx` | partial | 3 | 100,0 | 100,0 | 0 | 33 |
| `xlsx/muestras/inventario_regiones.xlsx` | complete | 247 | 99,19 | 99,19 | 2 | 391 |
| `xlsx/muestras/libro_con_perdidas.xlsx` | partial | 28 | 100,0 | 100,0 | 0 | 313 |
| `xlsx/muestras/ventas_trimestrales.xlsx` | complete | 3772 | 99,84 | 99,84 | 6 | 790 |

Lo que falta: en `estructura.docx`, la ecuación se escribe linealizada (`E=mc^2`, una palabra donde la referencia ve `m` y `c`) y el resultado del campo `PAGE` se sustituye por `[página]`, porque el número guardado no significa nada fuera de la paginación de Word; en `anexos_con_perdidas.docx`, el texto alternativo de un gráfico sin datos (§4); en los PDF, el encabezado o pie registrado una vez y palabras partidas con guion; en XLSX, valores que el bundle muestra con su formato (un porcentaje, o el formato General de Excel con 15 cifras significativas) y no como el número almacenado completo.

## 4. Hallazgos

- **DOCX: texto alternativo de una imagen ausente.** Cuando la imagen falta en el paquete, el bundle escribía solo «Imagen ausente del paquete original» y perdía su texto alternativo. Corregido: ahora lo añade («Texto alternativo: …»).
- **DOCX: texto alternativo de un gráfico sin datos.** Sigue perdiéndose: el lector no conserva el texto alternativo de los gráficos. Pendiente.
- **PDF: guiones legítimos al final de línea.** Corregido en el perfil 1.2.0 (7 de noviembre). La regla suprimía el guion de fin de línea entre minúsculas cuando la palabra compuesta no aparecía con guion en otra línea. En `p4.pdf` eso convierte `langchain-ai.github.io` en `langchainai.github.io` e `if-then-else` en `if-thenelse`. Mejoras posibles: conservar el guion dentro de URL y cuando la primera parte ya contiene un guion. Requiere cambiar la regla de la especificación.
- **XLSX: valor almacenado en tablas pequeñas.** Las tablas Markdown muestran el valor con su formato y el número almacenado completo solo está en los registros por celda de las regiones grandes. Se revisará con el primer libro real.

## 5. Repetir la medición

```bash
cargo build --release --locked
uv run tools/coverage.py --write-reference examples/*/corpus/* examples/*/muestras/* examples/pptx/*.pptx examples/docx/*.docx
```

Sin `--write-reference` el script solo informa. Con `--json RUTA` guarda también las 25 palabras que más faltan y más sobran de cada documento.

## 6. Control de regresión en Rust (plan 2, C2)

`cargo test --locked` ejecuta `src/coverage.rs` sobre los 27 documentos de `examples/`.
Lee las listas TSV independientes, convierte de nuevo cada documento y cuenta las
palabras de los mismos archivos y campos que `tools/coverage.py`: NFKC, eliminación
del guion blando, casefold completo y secuencias Unicode de letras y números. Compara
multiconjuntos; no usa la métrica más permisiva de reagrupación «Texto (%)».

`escaneado_ocr.pdf` es solo imagen: pdfium no extrae texto y su lista está vacía a
propósito, declarado con `"empty_reference": true` en los umbrales; cualquier otra lista
vacía hace fallar la prueba. Esta medición no cubre el OCR, que se mide contra el
documento digital del que procede la muestra (prueba
`ocr_transcribes_scanned_pages_through_the_common_layout` y [OCR local](OCR_LOCAL.md)).

`examples/coverage-thresholds.json` guarda la cobertura medida y el mínimo de cada
documento: la cifra publicada, redondeada a dos decimales, menos **0,50 puntos
porcentuales**. Conservamos las listas completas y las cifras de esta medición,
con las diferencias explicadas en §2 y §3; no se introducen descuentos adicionales
ni se ajusta automáticamente el umbral. Esta es la concreción de C2 solicitada
para el corte del 2 de noviembre.

El control es independiente de los golden: `OKF_UPDATE_GOLDEN=1` no modifica las
listas ni los umbrales. Una prueba de pérdida de texto comprueba esa separación.
Añadir un ejemplo exige añadir su lista independiente y su umbral; eliminar o dejar
huérfano un umbral también falla. Las actualizaciones de listas o mínimos requieren
una nueva medición y la revisión explícita de su diferencia.

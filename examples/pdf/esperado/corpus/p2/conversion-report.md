---
converter_version: "0.1.0"
converter_commit: "<commit>"
profile_version: "1.4.0"
conversion_settings:
  max_image_pixels: 33554432
  ocr:
    enabled: false
settings_sha256: "ad1d7709796a5172b6ad18d538b1bef5780b44279dd787d15ecc82ede959ef16"
conversion_id: "65e574ead417397d01694e1261ccc643cd282e5d26c89a767236a8303854cb16"
type: "Conversion Report"
unit_id: "65e574ead417397d01694e1261ccc643cd282e5d26c89a767236a8303854cb16/conversion-report.md"
title: "Informe de conversión"
generated:
  by: "process:document-conversion"
  at: "<generado>"
status: "complete"
format: "pdf"
sha256: "49ec606d9b25c9775c58749a7246c0451ba962124c8d34890815a011603ff015"
---

# Informe de conversión PDF

- Estado: `complete`
- Páginas: 9 (texto digital: 8; mixtas: 1; escaneadas: 0; escaneadas con OCR previo: 0; solo visuales: 0; en blanco: 0)
- Unidades representadas: 98 (párrafos, listas, tablas, imágenes y notas de página)
- Conceptos: 9 concepto(s), divididos por los encabezados de nivel 2
- Encabezados: 9 (de los marcadores del PDF)
- Tablas reconstruidas: 1; imágenes adjuntas: 3; vistas de página: 0
- Duración: <duración> ms

## Cobertura de páginas

Toda página física figura en algún concepto; las páginas sin texto propio llevan una nota (y su vista, si tienen contenido gráfico).

| Páginas | Clase | Conceptos |
| --- | --- | --- |
| 1 | texto digital | seccion-001.md, seccion-002.md, seccion-003.md |
| 2 | texto digital | seccion-003.md, seccion-004.md |
| 3 | texto digital | seccion-004.md, seccion-005.md |
| 4 | mixta (texto e imagen) | seccion-005.md |
| 5–6 | texto digital | seccion-005.md |
| 7 | texto digital | seccion-006.md, seccion-007.md |
| 8 | texto digital | seccion-007.md, seccion-008.md, seccion-009.md |
| 9 | texto digital | seccion-009.md |

## Incidencias

Pérdidas o incertidumbres sustantivas; determinan el estado `partial`.

Sin pérdidas ni incertidumbres sustantivas.

## Observaciones

Información sobre la conversión que no afecta el estado.

- Página 4: texto con imágenes grandes; las imágenes se adjuntan sin transcribir (no se aplicó OCR)
- Páginas 2, 3, 6 y 8: varias columnas; el orden de lectura se infiere de la disposición (columna por columna, de izquierda a derecha)
- 7 párrafo(s) unidos con su continuación en otra columna o página (la frase sigue en minúscula con el mismo formato): páginas 1, 1→2, 2, 2→3, 4, 5, 6
- 108 palabra(s) partidas con guion al final de línea, unidas sin el guion
- Página 8: tabla reconstruida por la alineación de sus celdas; puede no coincidir con las líneas de la tabla original
- Encabezados tomados de los marcadores del PDF (9 de 9 encontrados en el texto)
- Encabezados y pies de página registrados una vez en documento.md: 3 texto(s) y 0 imagen(es) repetidas
- Números de página impresos detectados en 8 página(s) y registrados en \`page\_labels\`
- 2 imagen(es) de menos del 1 % de la página consideradas decorativas; no se adjuntan
- Título del documento tomado del primer encabezado de la primera página, único de su nivel y de rango superior a los demás: «Artificial intelligence‑assisted academic writing: recommendations for ethical use»

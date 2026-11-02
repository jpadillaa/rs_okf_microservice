---
converter_version: "0.1.0"
converter_commit: "<commit>"
profile_version: "1.0.0"
type: "Conversion Report"
title: "Informe de conversión"
generated:
  by: "process:document-conversion"
  at: "<generado>"
status: "partial"
format: "pdf"
sha256: "eb53d8bd714184e9e2feb227d834eec03633bf7b54ab36799b43174d23376bcd"
---

# Informe de conversión PDF

- Estado: `partial`
- Páginas: 5 (texto digital: 1; mixtas: 1; escaneadas: 1; escaneadas con OCR previo: 0; solo visuales: 1; en blanco: 1)
- Unidades representadas: 10 (párrafos, listas, tablas, imágenes y notas de página)
- Conceptos: 1 concepto (documento breve o sin secciones)
- Encabezados: 2 (inferidos por tamaño de letra: 2)
- Tablas reconstruidas: 0; imágenes adjuntas: 1; vistas de página: 2
- Duración: <duración> ms

## Cobertura de páginas

Toda página física figura en algún concepto; las páginas sin texto propio llevan una nota (y su vista, si tienen contenido gráfico).

| Páginas | Clase | Conceptos |
| --- | --- | --- |
| 1 | texto digital | texto.md |
| 2 | escaneo sin texto | texto.md |
| 3 | en blanco | texto.md |
| 4 | solo visual | texto.md |
| 5 | mixta (texto e imagen) | texto.md |

## Incidencias

Pérdidas o incertidumbres sustantivas; determinan el estado `partial`.

- Página 2: escaneada sin capa de texto; se adjunta su vista y no se aplicó OCR

## Observaciones

Información sobre la conversión que no afecta el estado.

- Página 4: sin texto, con contenido gráfico; se adjunta su vista
- Página 5: texto con imágenes grandes; las imágenes se adjuntan sin transcribir (no se aplicó OCR)
- Página 3: en blanco o solo con encabezado, pie o número de página
- Página 1: texto girado, escrito al final del contenido de su página
- Página 5: encabezado inferido por su tamaño de letra (mayor que el texto corriente); el nivel sale del tamaño
- Números de página impresos detectados en 4 página(s) y registrados en \`page\_labels\`
- El PDF declara etiquetas internas (/PageLabels), listadas en documento.md; no sustituyen a los números impresos
- Título del documento tomado del primer encabezado de la primera página, único de su nivel y de rango superior a los demás: «Expediente digitalizado»

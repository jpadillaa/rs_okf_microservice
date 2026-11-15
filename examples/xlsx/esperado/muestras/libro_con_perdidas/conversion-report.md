---
converter_version: "0.1.0"
converter_commit: "<commit>"
profile_version: "1.23.0"
conversion_settings: {}
settings_sha256: "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
conversion_id: "618f28d1da12e44ef4ecf7f5e4d2002f772b804e15596e0a1de5d696e5ae9062"
type: "Conversion Report"
unit_id: "618f28d1da12e44ef4ecf7f5e4d2002f772b804e15596e0a1de5d696e5ae9062/conversion-report.md"
title: "Informe de conversión"
generated:
  by: "process:document-conversion"
  at: "<generado>"
status: "partial"
format: "xlsx"
sha256: "782106213febbcffae20d3f21c50c284a0b924d9afae2158ffecce866faf62e8"
---

# Informe de conversión XLSX

- Estado: `partial`
- Hojas inventariadas: 2 (0 de gráfico; ocultas: 0)
- Hojas representadas: 2 (un concepto por hoja)
- Celdas con contenido: 19; fórmulas: 10
- Regiones: 3 (en el concepto de su hoja: 3; con concepto propio: 0)
- Tablas de Excel: 0; tablas dinámicas: 0
- Recursos de datos: 0 CSV y 0 de registros por celda
- Gráficos: 1; imágenes adjuntas: 0
- Rangos de celdas combinadas: 0
- Comentarios de revisión: 0
- Nombres definidos: 1; vínculos externos: 1
- Duración de ensamblado: <duración> ms

## Referencias externas

Destinos fuera del bundle enlazados desde los conceptos, con los conceptos que los enlazan; no se comprueban ni se descargan. Las imágenes vinculadas, los libros externos y los objetos incrustados se declaran en las incidencias.

Sin enlaces externos.

## Incidencias

Pérdidas o incertidumbres sustantivas; determinan el estado `partial`.

- Hoja «Precios»: gráfico «Gráfico 1» (anclado en F1) sin datos extraíbles
- Hoja «Precios»: imagen (anclada en F17) ausente del paquete original
- Hoja «Precios»: objeto incrustado (Word.Document.12) sin representación fiel
- Hoja «Precios»: fórmulas sin valor almacenado en C2:C4 y D2:D4; el conversor no recalcula
- Hoja «Precios»: fórmulas en B2:B4 que leen el libro externo \[1\] (file:///C:/Compartido/Tarifas/precios\_2025.xlsx); conservan su último valor almacenado y el libro externo no está incluido
- Vínculo externo \[1\] al libro file:///C:/Compartido/Tarifas/precios\_2025.xlsx: no está incluido en el bundle

## Observaciones

Información sobre la conversión que no afecta el estado.

- Hoja «Precios»: formato numérico #,##0.00\\ "€" en B2:B4: el valor mostrado no incluye su unidad o texto
- El libro pide recalcular al abrirse (fullCalcOnLoad): los valores almacenados pueden no estar actualizados; el conversor no recalcula
- Título del libro tomado del nombre del archivo; el libro no lo declara

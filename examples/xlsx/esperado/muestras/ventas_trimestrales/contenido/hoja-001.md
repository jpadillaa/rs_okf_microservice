---
type: "Sheet"
unit_id: "adeb883395b672b24eea993c76db2907fdb48075a0ab8fd76462c4bc94268e1a/contenido/hoja-001.md"
parent: "documento.md"
next: "contenido/hoja-002.md"
title: "Resumen"
description: "Hoja 1 del libro ventas_trimestrales.xlsx: rango A1:F12 con 2 regiones."
sources:
  - id: "original"
    resource: "/references/original.xlsx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
sheet_index: 1
source_spans:
  - id: "hoja"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Resumen"
    range: "A1:F12"
  - id: "r-1"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Resumen"
    range: "A1:F8"
  - id: "r-2"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Resumen"
    range: "A10:A12"
---

# Resumen

<!-- okf-generated -->
Hoja 1 de 4 · rango con contenido A1:F12 (12 filas × 6 columnas) · 40 celdas con contenido, 9 fórmulas · 2 regiones.

Los valores de las fórmulas son los almacenados en el archivo; el conversor no recalcula.

## Resumen de ventas 2026 (A1:F8)

<!-- okf-source-span: r-1 -->
| Fila | Región | T1 | T2 | T3 | T4 | Variación T4/T1 |
| --- | --- | --- | --- | --- | --- | --- |
| 4 | Andina | 120500 | 131200 | 128900 | 140250 | 16.3900414937759% |
| 5 | Caribe | 98300 | 102100 | 99800 | 110400 | 12.3092573753815% |
| 6 | Pacífico | 76450 | 80100 | 85320 | 90010 | 17.7370830608241% |
| 7 | Orinoquía | 43200 | 41950 | 45600 | 47800 | 10.6481481481481% |
| 8 | Total | 338450 | 355350 | 359620 | 388460 | 14.7761855517802% |

**Fórmulas**

- F4:F8: `=E4/B4-1` en F4; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (F5: `=E5/B5-1`)
- B8:E8: `=SUM(B4:B7)` en B8; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (C8: `=SUM(C4:C7)`)

## Región A10:A12

<!-- okf-source-span: r-2 -->
- A10: Fuente: sistema comercial, corte al 15 de octubre de 2026.
- A11: [Ver detalle por operación](hoja-002.md)
- A12: [Tablero publicado](https://tablero.example.org/ventas-2026)

## Gráficos

- **Gráfico 1** (anclado en H2): Ventas por región y trimestre. T1: Andina = 120500; Caribe = 98300; Pacífico = 76450; Orinoquía = 43200. T2: Andina = 131200; Caribe = 102100; Pacífico = 80100; Orinoquía = 41950. T3: Andina = 128900; Caribe = 99800; Pacífico = 85320; Orinoquía = 45600. T4: Andina = 140250; Caribe = 110400; Pacífico = 90010; Orinoquía = 47800. Datos de origen: `Resumen!$B$3`, `Resumen!$A$4:$A$7`, `Resumen!$B$4:$B$7`, `Resumen!$C$3`, `Resumen!$C$4:$C$7`, `Resumen!$D$3`, `Resumen!$D$4:$D$7`, `Resumen!$E$3`, `Resumen!$E$4:$E$7`.

## Celdas combinadas

El valor está en la primera celda de cada rango; las demás quedan vacías: A1:F1.

## Formato condicional

Reglas del original; el resultado visual no se reproduce.

- B4:E7: valor de celda \< 50000

## Comentarios de revisión

<!-- okf-generated -->
> Comentarios del archivo original; no forman parte del contenido presentado.

- **Marta Ruiz**: Incluye la venta extraordinaria a la cadena hotelera. (celda B4)
- **Andrés Pineda** · 2026-10-20: ¿El total de T4 ya incluye las devoluciones? (celda E8)
  - **Marta Ruiz** · 2026-10-21: Sí, están descontadas desde la versión 3 del informe.

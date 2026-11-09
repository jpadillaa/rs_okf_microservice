---
type: "Source Section"
unit_id: "4e39fd8d2a62581cda4e8233a7b2c5daeac007f03c360195bed005c3702526b7/contenido/seccion-004.md"
parent: "documento.md"
previous: "contenido/seccion-003.md"
next: "contenido/seccion-005.md"
title: "3. Plan de migración"
description: "Sección 4 de informe_tecnico.pdf: 3. Plan de migración. La migración se hará en dos fases para limitar el riesgo. Durante todo el proceso el sistema actual seguirá disponible en modo de solo lectura, de modo que las…"
sources:
  - id: "original"
    resource: "/references/original.pdf"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "seccion"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 2
  - id: "u-17"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 2
    boxes:
      - page: 1
        x0: 72.00
        top: 661.72
        x1: 528.92
        bottom: 685.72
      - page: 2
        x0: 72.00
        top: 72.53
        x1: 536.44
        bottom: 110.03
  - id: "u-18"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 72.00
        top: 127.10
        x1: 208.00
        bottom: 139.10
  - id: "u-19-21"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 78.00
        top: 146.22
        x1: 407.09
        bottom: 189.72
  - id: "u-22"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 72.00
        top: 209.80
        x1: 194.66
        bottom: 221.80
  - id: "u-23"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 72.00
        top: 228.92
        x1: 526.57
        bottom: 266.42
title_inferred: true
---

# 3. Plan de migración

<!-- okf-generated -->
> Encabezado inferido por su formato tipográfico: el PDF no lo marca como encabezado.

<!-- okf-source-span: u-17 -->
La migración se hará en dos fases para limitar el riesgo. Durante todo el proceso el sistema actual seguirá disponible en modo de solo lectura, de modo que las sedes puedan consultar el inventario aunque no registren movimientos nuevos durante las ventanas de mantenimiento programadas por el equipo de infraestructura con las sedes regionales y con el área de compras, que deberá aprobar cada ventana con al menos una semana de anticipación para reorganizar sus pedidos.

<!-- okf-source-span: u-18 -->
## 3.1 Fase de preparación

<!-- okf-source-span: u-19-21 -->
- Inventario de datos y limpieza de registros duplicados.
- Pruebas de carga con una copia anonimizada de la base de datos.
- Capacitación de los responsables de cada sede.

<!-- okf-source-span: u-22 -->
## 3.2 Fase de ejecución

<!-- okf-source-span: u-23 -->
La ejecución empieza un viernes por la noche con la copia final de los datos y termina el domingo con la verificación de saldos en cada sede. Si la verificación falla, se vuelve al sistema anterior siguiendo el procedimiento de reversión.


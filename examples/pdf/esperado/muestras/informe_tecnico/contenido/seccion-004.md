---
type: "Source Section"
unit_id: "a38e2b6d4bb9b0f1993c27bfa7a1394d0603e8bf2cd07c780877be96c0cb5642/contenido/seccion-004.md"
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
  - id: "p-1-2"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 2
  - id: "p-2"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
title_inferred: true
---

# 3. Plan de migración

> Encabezado inferido por su formato tipográfico: el PDF no lo marca como encabezado.

<!-- okf-source-span: p-1-2 -->
La migración se hará en dos fases para limitar el riesgo. Durante todo el proceso el sistema actual seguirá disponible en modo de solo lectura, de modo que las sedes puedan consultar el inventario aunque no registren movimientos nuevos durante las ventanas de mantenimiento programadas por el equipo de infraestructura con las sedes regionales y con el área de compras, que deberá aprobar cada ventana con al menos una semana de anticipación para reorganizar sus pedidos.

<!-- okf-source-span: p-2 -->
## 3.1 Fase de preparación

<!-- okf-source-span: p-2 -->
- Inventario de datos y limpieza de registros duplicados.
- Pruebas de carga con una copia anonimizada de la base de datos.
- Capacitación de los responsables de cada sede.

<!-- okf-source-span: p-2 -->
## 3.2 Fase de ejecución

<!-- okf-source-span: p-2 -->
La ejecución empieza un viernes por la noche con la copia final de los datos y termina el domingo con la verificación de saldos en cada sede. Si la verificación falla, se vuelve al sistema anterior siguiendo el procedimiento de reversión.


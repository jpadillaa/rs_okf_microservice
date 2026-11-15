---
type: "Source Section"
unit_id: "30b1bb3bdabb57904e70ca49768f3b0f22ef1459fc3cba83c95bfed9e9e20f5a/contenido/seccion-004.md"
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
block_spans: "/contenido/seccion-004.spans.json"
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


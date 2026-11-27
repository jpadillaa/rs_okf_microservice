---
type: "Source Section"
unit_id: "02a8e14a5bf082d58492c93abf0e62ecb4fea138b9b7edb3f7672fef52d4e7a1/contenido/seccion-006.md"
parent: "documento.md"
previous: "contenido/seccion-005.md"
next: "contenido/seccion-007.md"
title: "Cuándo un sistema puede considerarse un harness"
description: "Sección 6 de d3.docx: Cuándo un sistema puede considerarse un harness. Académicamente se plantean cuatro condiciones funcionales para distinguir un harness de componentes más limitados, como un SDK, una biblioteca o un plugin."
sources:
  - id: "original"
    resource: "/references/original.docx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "seccion"
    source_id: "original"
    kind: "docx_blocks"
    from: 38
    to: 47
block_spans: "/contenido/seccion-006.spans.json"
---

# Cuándo un sistema puede considerarse un harness

<!-- okf-source-span: b-39 -->
Académicamente se plantean cuatro condiciones funcionales para distinguir un harness de componentes más limitados, como un SDK, una biblioteca o un plugin.

<!-- okf-source-span: b-40-43 -->
- **La primera condición es que el sistema cierre el ciclo de observación, razonamiento y acción**. El resultado de cada acción debe incorporarse al estado y afectar la decisión siguiente. Un componente que solo envía una solicitud al modelo y devuelve la respuesta no constituye un harness.
- **La segunda condición es que pueda ejecutar acciones concretas sobre un entorno externo**. Estas acciones pueden incluir consultar una base de datos, modificar un archivo, invocar una API, ejecutar código o actualizar un sistema corporativo. La generación exclusiva de texto no es suficiente.
- **La tercera condición es que administre de forma explícita el contexto y el estado**. Debe controlar qué información se incorpora a la ventana del modelo, cómo se compacta el historial, qué información se recupera desde fuentes externas y qué estado debe conservarse mediante memoria persistente entre pasos o sesiones.
- La cuarta condición es que imponga restricciones arquitectónicas sobre la ejecución. Esto incluye esquemas de herramientas, validaciones, presupuestos, condiciones de terminación, circuit breakers, políticas de autorización y mecanismos de aprobación humana.

<!-- okf-source-span: b-44 -->
Estas condiciones proporcionan una definición operacional útil, aunque no deben interpretarse como una taxonomía universalmente aceptada. El concepto de harness continúa siendo un área emergente de práctica e investigación.

<!-- okf-source-span: b-45 -->
Los cinco atributos presentados en la lectura anterior permiten clasificar un sistema como agéntico. Las cuatro condiciones descritas en esta lectura permiten determinar si su infraestructura de ejecución constituye un harness.

<!-- okf-source-span: b-46 -->
![Imagen 4](../assets/imagen-004.png)

<!-- okf-source-span: b-47 -->
**Figura 3.** Condiciones para distinguir un harness de un SDK.

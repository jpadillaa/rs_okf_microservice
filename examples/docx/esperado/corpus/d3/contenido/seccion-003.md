---
type: "Source Section"
unit_id: "77f91121bfa3e64fc312a8b8705bb9b624de367643298403fccfd9e11ea1734d/contenido/seccion-003.md"
title: "¿Qué es un harness?"
description: "Sección 3 de d3.docx: ¿Qué es un harness?. Un harness es la capa integral de software en tiempo de ejecución que rodea al modelo de lenguaje y gobierna su interacción con el estado, las herramientas y…"
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
    from: 12
    to: 21
  - id: "b-13"
    source_id: "original"
    kind: "docx_blocks"
    from: 13
    to: 13
  - id: "b-14"
    source_id: "original"
    kind: "docx_blocks"
    from: 14
    to: 14
  - id: "b-15-16"
    source_id: "original"
    kind: "docx_blocks"
    from: 15
    to: 16
  - id: "b-17"
    source_id: "original"
    kind: "docx_blocks"
    from: 17
    to: 17
  - id: "b-18"
    source_id: "original"
    kind: "docx_blocks"
    from: 18
    to: 18
  - id: "b-19"
    source_id: "original"
    kind: "docx_blocks"
    from: 19
    to: 19
  - id: "b-20"
    source_id: "original"
    kind: "docx_blocks"
    from: 20
    to: 20
  - id: "b-21"
    source_id: "original"
    kind: "docx_blocks"
    from: 21
    to: 21
---

# ¿Qué es un harness?

<!-- okf-source-span: b-13 -->
Un harness es la capa integral de software en tiempo de ejecución que rodea al modelo de lenguaje y gobierna su interacción con el estado, las herramientas y el entorno. Su función es transformar las propuestas probabilísticas del modelo en acciones estructuradas, validadas, controladas y observables.

<!-- okf-source-span: b-14 -->
Desde la perspectiva de ejecución, un sistema agéntico puede entenderse como la combinación de un modelo de lenguaje y un harness.

<!-- okf-source-span: b-15-16 -->
- El modelo actúa como motor de razonamiento semántico: interpreta el objetivo, evalúa información, propone acciones y analiza resultados.
- El harness determina cómo esas propuestas se incorporan al flujo, qué herramientas pueden utilizarse, qué información recibe el modelo, qué estado debe conservarse y qué restricciones deben aplicarse antes de ejecutar una acción.

<!-- okf-source-span: b-17 -->
El harness no constituye un único componente. Es una arquitectura operacional compuesta por un orquestador, mecanismos de gestión de contexto, memoria persistente, integración de herramientas, guardrails, observabilidad y capacidades de evaluación.

<!-- okf-source-span: b-18 -->
En esta arquitectura, el **orquestador** es el componente central del harness. Gobierna el flujo de ejecución, controla las transiciones entre estados y decide cuándo invocar al modelo, cuándo ejecutar una herramienta, cuándo consultar la memoria, cuándo evaluar un resultado y cuándo detener o escalar la tarea.

<!-- okf-source-span: b-19 -->
Esta relación es importante: ***el harness no sustituye al orquestador. Lo contiene.***

<!-- okf-source-span: b-20 -->
![Imagen 3](../assets/imagen-003.png)

<!-- okf-source-span: b-21 -->
**Figura 2.** Relación entre el modelo de lenguaje, el harness y el entorno de ejecución.

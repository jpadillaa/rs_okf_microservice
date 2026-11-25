---
type: "Source Section"
unit_id: "2752b1d7e68d7ceb4acfcd2de50485928778db9947d49ef7d7c2c9aab31376b9/contenido/seccion-003.md"
parent: "documento.md"
previous: "contenido/seccion-002.md"
next: "contenido/seccion-004.md"
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
block_spans: "/contenido/seccion-003.spans.json"
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

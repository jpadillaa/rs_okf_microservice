---
type: "Source Section"
unit_id: "a42ed4f41a1be1893de014cf90d674b7ca5c484f6af5abdd20e82387b64c29ce/contenido/seccion-011.md"
title: "El harness como plano de control"
description: "Sección 11 de d3.docx: El harness como plano de control. La confiabilidad de un sistema agéntico no reside exclusivamente en el modelo. Surge de la interacción entre el motor de razonamiento semántico y la…"
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
    from: 131
    to: 136
  - id: "b-132"
    source_id: "original"
    kind: "docx_blocks"
    from: 132
    to: 132
  - id: "b-133"
    source_id: "original"
    kind: "docx_blocks"
    from: 133
    to: 133
  - id: "b-134"
    source_id: "original"
    kind: "docx_blocks"
    from: 134
    to: 134
  - id: "b-135"
    source_id: "original"
    kind: "docx_blocks"
    from: 135
    to: 135
  - id: "b-136"
    source_id: "original"
    kind: "docx_blocks"
    from: 136
    to: 136
---

# El harness como plano de control

<!-- okf-source-span: b-132 -->
La confiabilidad de un sistema agéntico no reside exclusivamente en el modelo. Surge de la interacción entre el motor de razonamiento semántico y la infraestructura que controla su ejecución.

<!-- okf-source-span: b-133 -->
El harness constituye el plano de control de esa infraestructura. Administra el ciclo de ejecución, el contexto, las herramientas, la memoria persistente, los guardrails y la evaluación. También determina cuánta autonomía recibe el agente y bajo qué condiciones puede ejercerla.

<!-- okf-source-span: b-134 -->
Evaluar un sistema agéntico exige evaluar conjuntamente el modelo y el harness. Cuando una falla corresponde al control del flujo, la gestión del contexto, la memoria persistente, la recuperación, la ejecución de herramientas o la seguridad, aumentar la capacidad del modelo rara vez corrige la causa arquitectónica.

<!-- okf-source-span: b-135 -->
Para el ingeniero de software, la pregunta relevante no es únicamente qué modelo debe utilizarse. También debe establecerse cómo se controla el bucle de ejecución, qué información recibe el modelo, qué acciones están autorizadas, cómo se verifican los resultados y qué ocurre cuando el sistema falla.

<!-- okf-source-span: b-136 -->
En esas decisiones se determina si el agente es únicamente una demostración funcional o un sistema capaz de operar de manera confiable en producción.

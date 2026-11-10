---
type: "Source Section"
unit_id: "975c271cd79fbdb18d87362e8281fe5f611d6b82292b5da89fa7650b2d9f74c0/contenido/seccion-004.md"
parent: "documento.md"
previous: "contenido/seccion-003.md"
next: "contenido/seccion-005.md"
title: "De dónde viene el término"
description: "Sección 4 de d3.docx: De dónde viene el término. En la ingeniería de software clásica, un test harness es el andamiaje utilizado para ejecutar una unidad de código bajo condiciones controladas y repetibles.…"
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
    from: 22
    to: 29
  - id: "b-23"
    source_id: "original"
    kind: "docx_blocks"
    from: 23
    to: 23
  - id: "b-24"
    source_id: "original"
    kind: "docx_blocks"
    from: 24
    to: 24
  - id: "b-25"
    source_id: "original"
    kind: "docx_blocks"
    from: 25
    to: 25
  - id: "b-26-28"
    source_id: "original"
    kind: "docx_blocks"
    from: 26
    to: 28
  - id: "b-29"
    source_id: "original"
    kind: "docx_blocks"
    from: 29
    to: 29
---

# De dónde viene el término

<!-- okf-source-span: b-23 -->
En la ingeniería de software clásica, **un** ***test harness*** **es el andamiaje utilizado para ejecutar una unidad de código bajo condiciones controladas y repetibles**. Proporciona controladores, simuladores, datos de prueba y mecanismos de observación para aislar el componente evaluado y verificar su comportamiento.

<!-- okf-source-span: b-24 -->
Cuando los modelos de lenguaje comenzaron a seleccionar herramientas, modificar archivos y tomar decisiones durante la ejecución, el término adquirió un significado más amplio. El componente bajo control ya no era una función determinista, sino un motor probabilístico capaz de proponer distintas acciones ante un mismo estado.

<!-- okf-source-span: b-25 -->
La evolución reciente del concepto puede organizarse, con fines pedagógicos, en tres fases.

<!-- okf-source-span: b-26-28 -->
- Entre 2022 y 2023, el trabajo se concentró principalmente en la ingeniería de prompts y en la experimentación con bucles autónomos.
- Entre 2024 y 2025, la atención se desplazó hacia la ingeniería de contexto. La pregunta dejó de ser únicamente cómo instruir al modelo y pasó a incluir qué información debía recibir en cada paso, cómo recuperar conocimiento previo y cómo evitar que la ventana de contexto se saturara. La recuperación aumentada, los almacenes de memoria a largo plazo y los mecanismos de persistencia del estado adquirieron mayor relevancia, aunque la lógica de control continuaba tratándose con frecuencia como una capa de integración no estructurada.
- Desde 2025, diferentes trabajos académicos y experiencias de implementación comenzaron a tratar el harness como una unidad explícita de diseño. Los resultados obtenidos en entornos de evaluación de agentes de software, como SWE-bench, mostraron que modificar la infraestructura de ejecución, manteniendo constante el modelo, podía producir mejoras sustanciales. Esto no implica que el modelo sea irrelevante, sino que una proporción importante del desempeño observable depende de cómo se administran el contexto, las herramientas, el estado, la evaluación y las condiciones de terminación.

<!-- okf-source-span: b-29 -->
La ingeniería de prompts, la ingeniería de contexto y la ingeniería de harness no son etapas mutuamente excluyentes. Operan en niveles diferentes del mismo sistema.

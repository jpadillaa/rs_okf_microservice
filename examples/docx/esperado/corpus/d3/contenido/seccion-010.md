---
type: "Source Section"
unit_id: "975c271cd79fbdb18d87362e8281fe5f611d6b82292b5da89fa7650b2d9f74c0/contenido/seccion-010.md"
parent: "documento.md"
previous: "contenido/seccion-009.md"
next: "contenido/seccion-011.md"
title: "Principios para construir un harness"
description: "Sección 10 de d3.docx: Principios para construir un harness. Diseñar un harness implica desplazar la atención desde la calidad aislada del prompt hacia la confiabilidad del sistema completo. El objetivo no consiste en…"
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
    from: 116
    to: 130
  - id: "b-117"
    source_id: "original"
    kind: "docx_blocks"
    from: 117
    to: 117
  - id: "b-119"
    source_id: "original"
    kind: "docx_blocks"
    from: 119
    to: 119
  - id: "b-120"
    source_id: "original"
    kind: "docx_blocks"
    from: 120
    to: 120
  - id: "b-122"
    source_id: "original"
    kind: "docx_blocks"
    from: 122
    to: 122
  - id: "b-123-126"
    source_id: "original"
    kind: "docx_blocks"
    from: 123
    to: 126
  - id: "b-127"
    source_id: "original"
    kind: "docx_blocks"
    from: 127
    to: 127
  - id: "b-129"
    source_id: "original"
    kind: "docx_blocks"
    from: 129
    to: 129
  - id: "b-130"
    source_id: "original"
    kind: "docx_blocks"
    from: 130
    to: 130
---

# Principios para construir un harness

<!-- okf-source-span: b-117 -->
Diseñar un harness implica desplazar la atención desde la calidad aislada del prompt hacia la confiabilidad del sistema completo. El objetivo no consiste en asumir que el modelo dejará de cometer errores, sino en lograr que esos errores sean detectables, recuperables y limitados.

## Separación entre razonamiento y ejecución

<!-- okf-source-span: b-119 -->
El modelo debe tratarse como un motor de razonamiento semántico que propone acciones. No debe modificar directamente archivos, ejecutar consultas de red ni realizar transacciones. Cada propuesta debe atravesar una capa de validación que compruebe estructura, permisos, políticas y condiciones del estado actual. Solo después de superar estos controles puede ejecutarse en el entorno.

<!-- okf-source-span: b-120 -->
Esta separación evita que una salida probabilística se convierta automáticamente en una acción con efectos reales.

## Seguridad en las fronteras

<!-- okf-source-span: b-122 -->
La seguridad no debe depender de que el modelo reconozca todas las instrucciones maliciosas o recuerde permanentemente sus restricciones. Los controles deben ubicarse en las fronteras del sistema:

<!-- okf-source-span: b-123-126 -->
- antes de incorporar datos al contexto,
- antes de invocar una herramienta,
- antes de entregar información sensible y
- antes de confirmar una acción irreversible.

<!-- okf-source-span: b-127 -->
El harness debe asumir que el modelo puede recibir contenido no confiable, interpretar incorrectamente una instrucción o ser afectado por una inyección de prompt. Por tanto, las políticas de acceso y las validaciones deben operar independientemente de la interpretación del modelo.

## Externalización del control

<!-- okf-source-span: b-129 -->
El sistema debe utilizar señales externas y verificables para evaluar el progreso. Un agente de desarrollo puede ejecutar pruebas, compiladores y linters. Un agente financiero puede verificar reglas contables y límites transaccionales. Un agente de soporte puede consultar el estado real de un ticket.

<!-- okf-source-span: b-130 -->
Estas señales permiten contrastar las propuestas del modelo con evidencia empírica. Cuando una herramienta falla repetidamente, el consumo supera un presupuesto o el estado no muestra progreso, un circuit breaker debe interrumpir la ejecución. La estrategia correcta no es permitir que el modelo se autocorrija indefinidamente, sino establecer condiciones explícitas de reintento, terminación y escalamiento.

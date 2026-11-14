---
type: "Source Section"
unit_id: "fe14e3b3fdd7d84b581871fb2d5f604bd49884b7e334cde5419f0b24a76dae3f/contenido/seccion-009.md"
parent: "documento.md"
previous: "contenido/seccion-008.md"
next: "contenido/seccion-010.md"
title: "El doble paradigma"
description: "Sección 9 de d3.docx: El doble paradigma. Los sistemas agénticos operan bajo una tensión fundamental: el modelo de lenguaje es probabilístico, pero los procesos de negocio necesitan límites y garantías…"
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
    from: 97
    to: 115
  - id: "b-98"
    source_id: "original"
    kind: "docx_blocks"
    from: 98
    to: 98
  - id: "b-99"
    source_id: "original"
    kind: "docx_blocks"
    from: 99
    to: 99
  - id: "b-100"
    source_id: "original"
    kind: "docx_blocks"
    from: 100
    to: 100
  - id: "b-101"
    source_id: "original"
    kind: "docx_blocks"
    from: 101
    to: 101
  - id: "b-102"
    source_id: "original"
    kind: "docx_blocks"
    from: 102
    to: 102
  - id: "b-104"
    source_id: "original"
    kind: "docx_blocks"
    from: 104
    to: 104
  - id: "b-105-106"
    source_id: "original"
    kind: "docx_blocks"
    from: 105
    to: 106
  - id: "b-107"
    source_id: "original"
    kind: "docx_blocks"
    from: 107
    to: 107
  - id: "b-108"
    source_id: "original"
    kind: "docx_blocks"
    from: 108
    to: 108
  - id: "b-109-114"
    source_id: "original"
    kind: "docx_blocks"
    from: 109
    to: 114
  - id: "b-115"
    source_id: "original"
    kind: "docx_blocks"
    from: 115
    to: 115
---

# El doble paradigma

<!-- okf-source-span: b-98 -->
Los sistemas agénticos operan bajo una tensión fundamental: ***el modelo de lenguaje es probabilístico, pero los procesos de negocio necesitan límites y garantías explícitas***.

<!-- okf-source-span: b-99 -->
Un modelo puede interpretar una situación, proponer una herramienta y adaptar su estrategia ante resultados inesperados. Esta flexibilidad es valiosa para tareas abiertas y ambiguas. Sin embargo, el modelo no garantiza que seguirá la misma trayectoria ante entradas equivalentes ni que seleccionará siempre una acción permitida.

<!-- okf-source-span: b-100 -->
El harness introduce control determinista alrededor de ese razonamiento probabilístico. Define qué transiciones son posibles, qué herramientas están disponibles, qué esquemas deben cumplirse, qué permisos se aplican y cuándo debe detenerse la ejecución.

<!-- okf-source-span: b-101 -->
La separación entre razonamiento y ejecución es la expresión arquitectónica de este doble paradigma. El modelo propone una acción estructurada. El harness valida la propuesta, verifica los permisos, aplica las políticas y ejecuta la acción mediante la infraestructura correspondiente.

<!-- okf-source-span: b-102 -->
Esto no significa que toda acción externa sea determinista. Una API puede fallar, una red puede interrumpirse y un sistema externo puede cambiar de estado. Lo determinista corresponde a las reglas de control aplicadas por el harness, no necesariamente al resultado del entorno.

## Agencia condicionada y agencia mínima

<!-- okf-source-span: b-104 -->
El harness es el principal mecanismo arquitectónico mediante el cual se implementan la agencia condicionada y el principio de agencia mínima.

<!-- okf-source-span: b-105-106 -->
- La agencia condicionada establece que un agente puede actuar de forma independiente, pero únicamente dentro de límites explícitos de permisos, políticas, seguridad y presupuesto.
- La agencia mínima establece que el agente debe recibir la menor autonomía necesaria para cumplir su objetivo. Cada herramienta, permiso y transición adicional amplía simultáneamente la capacidad funcional y la superficie de ataque.

<!-- okf-source-span: b-107 -->
Estos principios deben implementarse en la infraestructura, no depender exclusivamente del prompt. Un modelo puede recibir una instrucción que le prohíba eliminar archivos, pero esa restricción es insuficiente si la herramienta expuesta permite ejecutar cualquier comando del sistema operativo.

<!-- okf-source-span: b-108 -->
El harness debe aplicar la restricción en varias capas.

<!-- okf-source-span: b-109-114 -->
1. En el orquestador, las transiciones determinan qué acciones pueden proponerse en cada estado.
2. En el registro de herramientas, los esquemas limitan los parámetros permitidos.
3. En la capa de autorización, las políticas determinan qué identidad puede ejecutar cada operación.
4. En el runtime, el aislamiento limita los recursos y sistemas accesibles.
5. En los guardrails, las validaciones comprueban que cada acción cumpla las reglas de negocio.
6. En los circuit breakers, los umbrales restringen el tiempo, los reintentos y el consumo computacional.

<!-- okf-source-span: b-115 -->
La autonomía no debe inferirse a partir de la capacidad del modelo. Debe concederse de manera explícita mediante la arquitectura.

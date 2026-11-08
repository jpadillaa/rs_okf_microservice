---
type: "Source Section"
unit_id: "4a2e1fcc8e388e6df1808e67b6d09b23fe33a8ac6eb95647e0a37ed650426f41/contenido/seccion-007.md"
parent: "documento.md"
previous: "contenido/seccion-006.md"
next: "contenido/seccion-008.md"
title: "El harness frente al resto de la arquitectura"
description: "Sección 7 de d3.docx: El harness frente al resto de la arquitectura. La arquitectura mínima presentada en la lectura de agentes describe cinco componentes funcionales: orquestador, modelo de lenguaje, herramientas, memoria y…"
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
    from: 48
    to: 60
  - id: "b-49"
    source_id: "original"
    kind: "docx_blocks"
    from: 49
    to: 49
  - id: "b-50-54"
    source_id: "original"
    kind: "docx_blocks"
    from: 50
    to: 54
  - id: "b-55"
    source_id: "original"
    kind: "docx_blocks"
    from: 55
    to: 55
  - id: "b-57"
    source_id: "original"
    kind: "docx_blocks"
    from: 57
    to: 57
  - id: "b-58-59"
    source_id: "original"
    kind: "docx_blocks"
    from: 58
    to: 59
  - id: "b-60"
    source_id: "original"
    kind: "docx_blocks"
    from: 60
    to: 60
---

# El harness frente al resto de la arquitectura

<!-- okf-source-span: b-49 -->
La arquitectura mínima presentada en la lectura de agentes describe cinco componentes funcionales: ***orquestador, modelo de lenguaje, herramientas, memoria y guardrails***. El concepto de harness no reemplaza esa arquitectura. La observa desde la perspectiva de la infraestructura de ejecución.

<!-- okf-source-span: b-50-54 -->
1. **El modelo de lenguaje se mantiene como motor de razonamiento semántico**. No ejecuta herramientas directamente, no conserva por sí mismo memoria persistente y no debe poseer autoridad final sobre las acciones realizadas en el entorno.
2. **El orquestador forma parte del harness y gobierna el flujo de ejecución**. En LangGraph, esta responsabilidad se representa mediante el grafo de estados, sus nodos, sus aristas y sus condiciones de transición.
3. **Las herramientas conectan al agente con sistemas externos**. El harness administra su descubrimiento, descripción, validación, autorización, invocación y observación.
4. **La memoria se distribuye entre diferentes componentes del harness**. La memoria a corto plazo se representa en el estado de ejecución y en la información incorporada a la ventana de contexto. La memoria a largo plazo se implementa mediante almacenes externos desde los cuales puede recuperarse información de interacciones anteriores. La memoria persistente garantiza que el estado de una tarea sobreviva a interrupciones, pausas o reinicios. El checkpointing es el mecanismo técnico mediante el cual se capturan y restauran snapshots de ese estado.
5. **Los guardrails se ejecutan en puntos específicos del harness**. Pueden validar entradas, salidas, argumentos de herramientas, permisos, presupuestos y condiciones de seguridad.

<!-- okf-source-span: b-55 -->
Por tanto, la arquitectura mínima del agente y la arquitectura interna del harness describen el mismo sistema desde dos niveles de abstracción diferentes. La primera identifica los componentes funcionales del sistema agéntico. La segunda explica cómo se gobierna su ejecución.

## Harness, orquestación y sistemas multiagente

<!-- okf-source-span: b-57 -->
El término orquestador no debe limitarse a sistemas compuestos por múltiples agentes. Todo sistema agéntico necesita algún mecanismo que controle su flujo, aunque solo utilice un modelo y un conjunto reducido de herramientas.

<!-- okf-source-span: b-58-59 -->
- En un agente individual, el orquestador decide qué nodo se ejecuta, cuándo debe invocarse el modelo, qué herramienta puede utilizarse y cuándo termina la tarea.
- En una arquitectura multiagente, el mismo concepto se amplía. El orquestador también puede distribuir tareas, enrutar solicitudes, administrar dependencias y coordinar la comunicación entre agentes especializados.

<!-- okf-source-span: b-60 -->
La orquestación multiagente es, por tanto, un caso particular de orquestación. No constituye la definición general del término.

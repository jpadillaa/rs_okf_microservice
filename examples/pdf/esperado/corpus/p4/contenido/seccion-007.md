---
type: "Source Section"
unit_id: "60f867d0d9d4911da4d8e656e26f567a2e6b239065e7f759a0c17f83389162f8/contenido/seccion-007.md"
parent: "documento.md"
previous: "contenido/seccion-006.md"
next: "contenido/seccion-008.md"
title: "El principio de agencia mínima"
description: "Sección 7 de p4.pdf: El principio de agencia mínima. El concepto más importante para un ingeniero que va a construir agentes no es técnico sino filosófico, aunque tiene consecuencias directas en cada línea de…"
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
    from: 11
    to: 12
  - id: "u-78"
    source_id: "original"
    kind: "pdf_pages"
    from: 11
    to: 11
    boxes:
      - page: 11
        x0: 85.03
        top: 121.55
        x1: 525.15
        bottom: 169.55
  - id: "u-79"
    source_id: "original"
    kind: "pdf_pages"
    from: 11
    to: 11
    boxes:
      - page: 11
        x0: 85.03
        top: 187.58
        x1: 524.94
        bottom: 271.58
  - id: "u-80"
    source_id: "original"
    kind: "pdf_pages"
    from: 11
    to: 11
    boxes:
      - page: 11
        x0: 85.03
        top: 289.60
        x1: 519.50
        bottom: 391.62
  - id: "u-81"
    source_id: "original"
    kind: "pdf_pages"
    from: 11
    to: 11
    boxes:
      - page: 11
        x0: 85.03
        top: 409.62
        x1: 524.98
        bottom: 529.67
  - id: "u-82"
    source_id: "original"
    kind: "pdf_pages"
    from: 11
    to: 11
    boxes:
      - page: 11
        x0: 85.03
        top: 547.67
        x1: 505.38
        bottom: 667.70
  - id: "u-83"
    source_id: "original"
    kind: "pdf_pages"
    from: 11
    to: 11
    boxes:
      - page: 11
        x0: 85.03
        top: 685.72
        x1: 524.46
        bottom: 715.72
  - id: "u-84"
    source_id: "original"
    kind: "pdf_pages"
    from: 12
    to: 12
    boxes:
      - page: 12
        x0: 85.03
        top: 76.53
        x1: 520.27
        bottom: 142.55
  - id: "u-85"
    source_id: "original"
    kind: "pdf_pages"
    from: 12
    to: 12
    boxes:
      - page: 12
        x0: 85.03
        top: 160.55
        x1: 523.72
        bottom: 280.60
---

# El principio de agencia mínima

<!-- okf-source-span: u-78 -->
El concepto más importante para un ingeniero que va a construir agentes no es técnico sino filosófico, aunque tiene consecuencias directas en cada línea de código: **la agencia mínima** (Least Agency).

<!-- okf-source-span: u-79 -->
El principio proviene del marco *AEGIS de Forrester* y fue adoptado por la *OWASP Top 10 for Agentic Applications 2026* como principio operativo fundamental. Su formulación es directa, *un agente debe recibir la mínima autonomía necesaria para cumplir su objetivo, y esa autonomía debe ser otorgada explícitamente por el diseñador, no inferida por el modelo*.

<!-- okf-source-span: u-80 -->
Este principio es una evolución del *Principio de Privilegio Mínimo (Least Privilege)* que cualquier ingeniero de sistemas conoce. El privilegio mínimo restringe a qué sistemas y datos puede acceder una identidad. La agencia mínima desciende a la capa de comportamiento: restringe qué decisiones puede tomar el agente, qué herramientas puede invocar dentro de un sistema autorizado y bajo qué condiciones lógicas o presupuestarias puede operar.

<!-- okf-source-span: u-81 -->
La distinción es operativamente crítica. Un agente puede tener acceso (privilegio) a una base de datos de clientes, pero la agencia mínima dicta que solo puede ejecutar consultas de lectura, no operaciones de escritura. Puede tener acceso a un sistema de pagos, pero la agencia mínima dicta que solo puede emitir reembolsos por debajo de un monto específico sin aprobación humana. Puede tener acceso a un servicio de correo electrónico, pero la agencia mínima dicta que solo puede enviar correos a direcciones internas de la organización.

<!-- okf-source-span: u-82 -->
La validación de estos límites debe aplicarse **independientemente de la comprensión que el propio agente tiene de sus permisos**. Esto significa que los guardrails no operan dentro del prompt del modelo (donde pueden ser eludidos mediante inyección) sino en la capa de infraestructura: en el orquestador, en los gateways de API, en las políticas de acceso de la base de datos. Si el modelo es engañado para creer que tiene permiso de ejecutar una acción destructiva, la infraestructura rechaza la transacción antes de que se materialice.

<!-- okf-source-span: u-83 -->
En LangGraph, la agencia mínima se implementa en tres capas. En la capa del grafo, las transiciones permitidas definen qué acciones puede proponer el agente en cada estado.

<!-- okf-source-span: u-84 -->
En la capa de herramientas, cada tool tiene un esquema de validación que rechaza invocaciones fuera de los parámetros esperados. En la capa de guardrails, funciones de validación verifican la salida del modelo contra políticas de negocio antes de que cualquier acción se ejecute.

<!-- okf-source-span: u-85 -->
La tentación del ingeniero software es otorgar autonomía máxima para maximizar la flexibilidad del sistema. El principio de agencia mínima invierte esa lógica, la autonomía es una característica que se otorga explícitamente cuando existe una justificación operativa clara, no una principio por defecto. Cada herramienta habilitada, cada permiso otorgado y cada decisión delegada al modelo es simultáneamente una capacidad y una superficie de ataque. El diseño responsable consiste en encontrar el balance mínimo entre utilidad y riesgo.


---
type: Source Section
title: "El principio de agencia mínima"
description: "Sección 7 de p4.pdf: El principio de agencia mínima. El concepto más importante para un ingeniero que va a construir agentes no es técnico sino filosófico, aunque tiene consecuencias directas en cada línea de…"
sources:
  - id: original
    resource: /references/original.pdf
generated:
  by: process:document-conversion
  at: "<generado>"
source_spans:
  - { id: seccion, source_id: original, kind: pdf_pages, from: 11, to: 12 }
  - { id: p-11, source_id: original, kind: pdf_pages, from: 11, to: 11 }
  - { id: p-12, source_id: original, kind: pdf_pages, from: 12, to: 12 }
---

# El principio de agencia mínima

<!-- okf-source-span: p-11 -->
El concepto más importante para un ingeniero que va a construir agentes no es técnico sino filosófico, aunque tiene consecuencias directas en cada línea de código: **la agencia mínima** (Least Agency).

<!-- okf-source-span: p-11 -->
El principio proviene del marco *AEGIS de Forrester* y fue adoptado por la *OWASP Top 10 for Agentic Applications 2026* como principio operativo fundamental. Su formulación es directa, *un agente debe recibir la mínima autonomía necesaria para cumplir su objetivo, y esa autonomía debe ser otorgada explícitamente por el diseñador, no inferida por el modelo*.

<!-- okf-source-span: p-11 -->
Este principio es una evolución del *Principio de Privilegio Mínimo (Least Privilege)* que cualquier ingeniero de sistemas conoce. El privilegio mínimo restringe a qué sistemas y datos puede acceder una identidad. La agencia mínima desciende a la capa de comportamiento: restringe qué decisiones puede tomar el agente, qué herramientas puede invocar dentro de un sistema autorizado y bajo qué condiciones lógicas o presupuestarias puede operar.

<!-- okf-source-span: p-11 -->
La distinción es operativamente crítica. Un agente puede tener acceso (privilegio) a una base de datos de clientes, pero la agencia mínima dicta que solo puede ejecutar consultas de lectura, no operaciones de escritura. Puede tener acceso a un sistema de pagos, pero la agencia mínima dicta que solo puede emitir reembolsos por debajo de un monto específico sin aprobación humana. Puede tener acceso a un servicio de correo electrónico, pero la agencia mínima dicta que solo puede enviar correos a direcciones internas de la organización.

<!-- okf-source-span: p-11 -->
La validación de estos límites debe aplicarse **independientemente de la comprensión que el propio agente tiene de sus permisos**. Esto significa que los guardrails no operan dentro del prompt del modelo (donde pueden ser eludidos mediante inyección) sino en la capa de infraestructura: en el orquestador, en los gateways de API, en las políticas de acceso de la base de datos. Si el modelo es engañado para creer que tiene permiso de ejecutar una acción destructiva, la infraestructura rechaza la transacción antes de que se materialice.

<!-- okf-source-span: p-11 -->
En LangGraph, la agencia mínima se implementa en tres capas. En la capa del grafo, las transiciones permitidas definen qué acciones puede proponer el agente en cada estado.

<!-- okf-source-span: p-12 -->
En la capa de herramientas, cada tool tiene un esquema de validación que rechaza invocaciones fuera de los parámetros esperados. En la capa de guardrails, funciones de validación verifican la salida del modelo contra políticas de negocio antes de que cualquier acción se ejecute.

<!-- okf-source-span: p-12 -->
La tentación del ingeniero software es otorgar autonomía máxima para maximizar la flexibilidad del sistema. El principio de agencia mínima invierte esa lógica, la autonomía es una característica que se otorga explícitamente cuando existe una justificación operativa clara, no una principio por defecto. Cada herramienta habilitada, cada permiso otorgado y cada decisión delegada al modelo es simultáneamente una capacidad y una superficie de ataque. El diseño responsable consiste en encontrar el balance mínimo entre utilidad y riesgo.


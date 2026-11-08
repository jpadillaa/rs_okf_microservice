---
type: "Source Section"
unit_id: "c269fbb666f1339ef8bf4e9207d15b75a008a0ef6915613b180f4fa6f3b4922f/contenido/seccion-005.md"
parent: "documento.md"
previous: "contenido/seccion-004.md"
next: "contenido/seccion-006.md"
title: "La arquitectura mínima de un sistema agéntico"
description: "Sección 5 de p4.pdf: La arquitectura mínima de un sistema agéntico. Un modelo de lenguaje puede interpretar objetivos, razonar sobre información y proponer acciones, pero esas capacidades probabilísticas no constituyen por sí…"
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
    from: 6
    to: 9
  - id: "p-6"
    source_id: "original"
    kind: "pdf_pages"
    from: 6
    to: 6
  - id: "p-6-7"
    source_id: "original"
    kind: "pdf_pages"
    from: 6
    to: 7
  - id: "p-7"
    source_id: "original"
    kind: "pdf_pages"
    from: 7
    to: 7
  - id: "p-7-8"
    source_id: "original"
    kind: "pdf_pages"
    from: 7
    to: 8
  - id: "p-8"
    source_id: "original"
    kind: "pdf_pages"
    from: 8
    to: 8
  - id: "p-9"
    source_id: "original"
    kind: "pdf_pages"
    from: 9
    to: 9
---

# La arquitectura mínima de un sistema agéntico

<!-- okf-source-span: p-6 -->
Un modelo de lenguaje puede interpretar objetivos, razonar sobre información y proponer acciones, pero esas capacidades probabilísticas no constituyen por sí solas un sistema agéntico confiable.

<!-- okf-source-span: p-6-7 -->
El harness es la arquitectura de control que convierte esas propuestas en una ejecución gobernada. Administra el flujo, incorpora el contexto necesario, conserva la memoria persistente, valida el uso de herramientas, aplica límites y coordina la interacción con el runtime y los sistemas externos.

<!-- okf-source-span: p-7 -->
De este modo, el agente no actúa porque el modelo tenga control directo sobre el entorno, sino porque existe un sistema que decide qué puede ejecutarse, cómo debe validarse y cuándo es necesario detenerse o solicitar intervención.

<!-- okf-source-span: p-7 -->
![Imagen de la página 7](../assets/pagina-007-imagen-01.png)

<!-- okf-source-span: p-7 -->
**Figura 1**. Arquitectura mínima de un agente

<!-- okf-source-span: p-7 -->
La figura 1 ilustra la estructura de componentes que todo sistema agéntico debe tener independientemente del proveedor o framework utilizado, se organiza en cinco capas funcionales.

<!-- okf-source-span: p-7-8 -->
1. **El orquestador** es el componente que gobierna el flujo. No es el LLM. El orquestador decide cuándo invocar al modelo, cuándo ejecutar una herramienta, cuándo consultar la memoria y cuándo aplicar un guardrail. En LangGraph, el orquestador es el grafo de estados con sus nodos y transiciones condicionales. La decisión de diseño más importante que tomará como ingeniero es cuánto control delega al LLM (razonamiento probabilístico) y cuánto retiene en el orquestador (lógica determinista).
2. **El modelo de lenguaje** actúa exclusivamente como motor de razonamiento semántico. Interpreta intenciones, genera texto, evalúa resultados y propone acciones. No controla el flujo, no ejecuta herramientas directamente y no persiste estado.
3. **Las herramientas** son las interfaces con el mundo externo. Cada herramienta es una función con un nombre, una descripción semántica (que el LLM usa para decidir cuándo invocarla) y un esquema de entrada y salida.
4. **La memoria** opera en los tres niveles ya descritos. En la implementación con *LangGraph*, la memoria a corto plazo es el estado del grafo (la lista de mensajes y variables del turno actual). La memoria a largo plazo se implementa conectando una base de datos vectorial como almacén de store. La memoria persistente se implementa mediante un checkpointer que serializa el estado del grafo en cada nodo.
5. **Los guardrails** son funciones de validación que se ejecutan en puntos específicos del flujo. Operan independientemente del LLM; incluso si el modelo es engañado mediante una inyección de prompt, los guardrails interceptan la acción antes de que se ejecute. En *LangGraph*, los guardrails se implementan como nodos de validación en el grafo o como funciones de transformación en las aristas.

<!-- okf-source-span: p-8 -->
La relación puede resumirse mediante el siguiente principio:

<!-- okf-source-span: p-8 -->
**El modelo propone. El harness controla. El runtime ejecuta. El entorno produce resultados.**

<!-- okf-source-span: p-9 -->
![Imagen de la página 9](../assets/pagina-009-imagen-01.png)

<!-- okf-source-span: p-9 -->
**Figura 2. La arquitectura mínima de un sistema agéntico.**

<!-- okf-source-span: p-9 -->
La figura 2 ilustra la estructura de componentes que debe incorporar un sistema agéntico confiable, independientemente del proveedor o framework utilizado.

<!-- okf-source-span: p-9 -->
El harness no es un componente único ni debe confundirse con el orquestador. Es una arquitectura integrada de control que reúne las responsabilidades necesarias para transformar las propuestas del modelo en una ejecución segura.

<!-- okf-source-span: p-9 -->
El orquestador es uno de los componentes centrales del harness. Su responsabilidad consiste en gobernar el flujo, mantener el estado de la ejecución y seleccionar la siguiente operación permitida. El harness también incluye la gestión de contexto, la memoria persistente, las políticas, los guardrails, la integración con herramientas y la coordinación con el runtime.


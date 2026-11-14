---
type: "Source Section"
unit_id: "fe14e3b3fdd7d84b581871fb2d5f604bd49884b7e334cde5419f0b24a76dae3f/contenido/seccion-005.md"
parent: "documento.md"
previous: "contenido/seccion-004.md"
next: "contenido/seccion-006.md"
title: "Lo que un harness no es"
description: "Sección 5 de d3.docx: Lo que un harness no es. El término harness se utiliza de manera ambigua en la industria. Con frecuencia se emplea como sinónimo de framework, runtime, sandbox o conjunto de…"
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
    from: 30
    to: 37
  - id: "b-31"
    source_id: "original"
    kind: "docx_blocks"
    from: 31
    to: 31
  - id: "b-32-37"
    source_id: "original"
    kind: "docx_blocks"
    from: 32
    to: 37
---

# Lo que un harness no es

<!-- okf-source-span: b-31 -->
El término harness se utiliza de manera ambigua en la industria. Con frecuencia se emplea como sinónimo de framework, runtime, sandbox o conjunto de herramientas. Estas equivalencias son imprecisas.

<!-- okf-source-span: b-32-37 -->
1. **Un harness no es únicamente un prompt complejo**. El prompt configura el comportamiento esperado del modelo, pero no controla por sí mismo los permisos, el estado, la ejecución de herramientas ni la recuperación frente a fallos.
2. **Un harness no es un framework**. Un framework como LangGraph proporciona abstracciones de software para implementar componentes del harness, como el grafo de ejecución, las transiciones condicionales, la memoria persistente y la integración de herramientas. El framework es una tecnología de implementación; ***el harness es una responsabilidad arquitectónica y operacional.***
3. **Un harness no es un runtime**. El runtime proporciona las condiciones materiales de ejecución, como los procesos, dependencias, recursos computacionales, aislamiento y acceso al entorno. Puede estar basado en procesos locales, contenedores, máquinas virtuales, funciones serverless o servicios gestionados. El harness determina qué debe ejecutarse y bajo qué condiciones; el runtime determina dónde y con qué recursos se ejecuta.
4. **Un harness no es un sandbox**. Un sandbox es un entorno aislado que puede utilizarse para ejecutar código, procesar archivos no confiables o contener herramientas de alto riesgo. Es un mecanismo de protección utilizado por el harness, no un sistema con capacidad de razonamiento o control del flujo.
5. **Un harness no es exclusivamente un registro de herramientas**. El registro mantiene el catálogo de funciones disponibles, pero no administra por sí mismo el ciclo completo de observación, razonamiento, acción, memoria y evaluación.
6. **Un harness tampoco es únicamente un harness de evaluación**. Un *evaluation harness* ejecuta pruebas y recopila métricas para comparar modelos o agentes. El harness de ejecución gobierna el comportamiento operacional del agente durante una tarea real. Ambos pueden compartir componentes de instrumentación, pero cumplen funciones diferentes.

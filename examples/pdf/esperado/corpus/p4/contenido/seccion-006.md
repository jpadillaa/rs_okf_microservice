---
type: Source Section
title: "El doble paradigma"
description: "Sección 6 de p4.pdf: El doble paradigma. Los sistemas agénticos corporativos operan bajo una tensión fundamental. El LLM es probabilístico, pero los procesos de negocio requieren controles…"
sources:
  - id: original
    resource: /references/original.pdf
generated:
  by: process:document-conversion
  at: "<generado>"
source_spans:
  - { id: seccion, source_id: original, kind: pdf_pages, from: 9, to: 10 }
  - { id: p-9, source_id: original, kind: pdf_pages, from: 9, to: 9 }
  - { id: p-10, source_id: original, kind: pdf_pages, from: 10, to: 10 }
---

# El doble paradigma

<!-- okf-source-span: p-9 -->
Los sistemas agénticos corporativos operan bajo una tensión fundamental. El LLM es probabilístico, pero los procesos de negocio requieren controles deterministas en los puntos donde una decisión puede producir efectos relevantes.

<!-- okf-source-span: p-10 -->
Un LLM que opera bajo el paradigma ReAct (Reasoning and Action) funciona mediante un ciclo de retroalimentación. Evalúa la situación, selecciona una herramienta, observa el resultado, razona sobre la nueva información y propone la siguiente acción. Este enfoque es flexible y puede manejar problemas abiertos y ambiguos.

<!-- okf-source-span: p-10 -->
Sin embargo, el comportamiento no es completamente predecible. No existe garantía de que el modelo siga exactamente la misma ruta dos veces ante el mismo problema, ni de que todas sus propuestas respeten por sí solas las restricciones operativas de la organización.

<!-- okf-source-span: p-10 -->
Para flujos críticos de negocio; transacciones financieras, procesamiento de datos regulados o decisiones con implicaciones legales; esta variabilidad no puede quedar sin control. La industria ha adoptado un doble paradigma que combina el razonamiento probabilístico del modelo con el control determinista del harness.

<!-- okf-source-span: p-10 -->
El modelo aporta interpretación semántica, adaptación y capacidad para trabajar con información ambigua. El harness establece las transiciones permitidas, valida las herramientas, aplica políticas, controla los presupuestos y determina cuándo una propuesta puede producir un efecto real.

<!-- okf-source-span: p-10 -->
La enseñanza fundamental es que diseñar agentes no consiste en rodear un modelo con herramientas ni en elegir un framework. Consiste en construir una frontera de ingeniería entre razonamiento y ejecución.

<!-- okf-source-span: p-10 -->
Esa frontera inspecciona las propuestas del modelo antes de materializarlas, restringe su autonomía y permite detectar, contener y recuperar errores antes de que produzcan efectos inaceptables.

<!-- okf-source-span: p-10 -->
Un agente bien diseñado no es un LLM al que se le entrega acceso directo a herramientas y se le ordena “resolver” una tarea. Es un sistema donde el modelo aporta capacidad semántica dentro de límites explícitos y donde el harness conserva la autoridad sobre la ejecución.

<!-- okf-source-span: p-10 -->
Construir esta frontera exige decidir cuánta autonomía puede recibir el modelo y qué acciones deben permanecer bajo control determinista. Esta decisión conduce al principio de agencia mínima.


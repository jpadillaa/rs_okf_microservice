---
type: "Source Section"
unit_id: "f98aad26887b3c3a6d1c53082e01a9b28e0553452136090e6609645ba3efca6d/contenido/seccion-006.md"
parent: "documento.md"
previous: "contenido/seccion-005.md"
next: "contenido/seccion-007.md"
title: "El doble paradigma"
description: "Sección 6 de p4.pdf: El doble paradigma. Los sistemas agénticos corporativos operan bajo una tensión fundamental. El LLM es probabilístico, pero los procesos de negocio requieren controles…"
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
    from: 9
    to: 10
  - id: "u-68"
    source_id: "original"
    kind: "pdf_pages"
    from: 9
    to: 9
    boxes:
      - page: 9
        x0: 85.03
        top: 634.70
        x1: 509.81
        bottom: 682.73
  - id: "u-69"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 76.53
        x1: 521.09
        bottom: 142.55
  - id: "u-70"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 160.55
        x1: 522.97
        bottom: 226.58
  - id: "u-71"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 244.58
        x1: 523.27
        bottom: 310.60
  - id: "u-72"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 328.60
        x1: 513.50
        bottom: 394.62
  - id: "u-73"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 412.62
        x1: 525.74
        bottom: 460.65
  - id: "u-74"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 478.65
        x1: 519.69
        bottom: 526.67
  - id: "u-75"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 544.67
        x1: 523.23
        bottom: 610.70
  - id: "u-76"
    source_id: "original"
    kind: "pdf_pages"
    from: 10
    to: 10
    boxes:
      - page: 10
        x0: 85.03
        top: 628.70
        x1: 509.70
        bottom: 676.72
---

# El doble paradigma

<!-- okf-source-span: u-68 -->
Los sistemas agénticos corporativos operan bajo una tensión fundamental. El LLM es probabilístico, pero los procesos de negocio requieren controles deterministas en los puntos donde una decisión puede producir efectos relevantes.

<!-- okf-source-span: u-69 -->
Un LLM que opera bajo el paradigma ReAct (Reasoning and Action) funciona mediante un ciclo de retroalimentación. Evalúa la situación, selecciona una herramienta, observa el resultado, razona sobre la nueva información y propone la siguiente acción. Este enfoque es flexible y puede manejar problemas abiertos y ambiguos.

<!-- okf-source-span: u-70 -->
Sin embargo, el comportamiento no es completamente predecible. No existe garantía de que el modelo siga exactamente la misma ruta dos veces ante el mismo problema, ni de que todas sus propuestas respeten por sí solas las restricciones operativas de la organización.

<!-- okf-source-span: u-71 -->
Para flujos críticos de negocio; transacciones financieras, procesamiento de datos regulados o decisiones con implicaciones legales; esta variabilidad no puede quedar sin control. La industria ha adoptado un doble paradigma que combina el razonamiento probabilístico del modelo con el control determinista del harness.

<!-- okf-source-span: u-72 -->
El modelo aporta interpretación semántica, adaptación y capacidad para trabajar con información ambigua. El harness establece las transiciones permitidas, valida las herramientas, aplica políticas, controla los presupuestos y determina cuándo una propuesta puede producir un efecto real.

<!-- okf-source-span: u-73 -->
La enseñanza fundamental es que diseñar agentes no consiste en rodear un modelo con herramientas ni en elegir un framework. Consiste en construir una frontera de ingeniería entre razonamiento y ejecución.

<!-- okf-source-span: u-74 -->
Esa frontera inspecciona las propuestas del modelo antes de materializarlas, restringe su autonomía y permite detectar, contener y recuperar errores antes de que produzcan efectos inaceptables.

<!-- okf-source-span: u-75 -->
Un agente bien diseñado no es un LLM al que se le entrega acceso directo a herramientas y se le ordena “resolver” una tarea. Es un sistema donde el modelo aporta capacidad semántica dentro de límites explícitos y donde el harness conserva la autoridad sobre la ejecución.

<!-- okf-source-span: u-76 -->
Construir esta frontera exige decidir cuánta autonomía puede recibir el modelo y qué acciones deben permanecer bajo control determinista. Esta decisión conduce al principio de agencia mínima.


---
type: "Source Section"
unit_id: "c030b4e259593c0bb2f19e839834a5a04a1e943f9c0899e925a7d1f69709c8c6/contenido/seccion-002.md"
parent: "documento.md"
previous: "contenido/seccion-001.md"
next: "contenido/seccion-003.md"
title: "¿Qué es un agente? La pregunta que importa"
description: "Sección 2 de p4.pdf: ¿Qué es un agente? La pregunta que importa. Antes de definir qué es un agente, vale la pena entender por qué necesitamos el concepto."
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
    from: 1
    to: 2
  - id: "u-4"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 85.03
        top: 427.65
        x1: 482.21
        bottom: 457.65
  - id: "u-5"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
    boxes:
      - page: 1
        x0: 85.03
        top: 475.65
        x1: 526.21
        bottom: 613.70
  - id: "u-6"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 2
    boxes:
      - page: 1
        x0: 85.03
        top: 631.70
        x1: 521.72
        bottom: 715.72
      - page: 2
        x0: 85.03
        top: 76.53
        x1: 513.94
        bottom: 142.55
  - id: "u-7"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 85.03
        top: 161.30
        x1: 298.10
        bottom: 173.30
  - id: "u-8"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 85.03
        top: 190.58
        x1: 525.99
        bottom: 274.57
  - id: "u-9"
    source_id: "original"
    kind: "pdf_pages"
    from: 2
    to: 2
    boxes:
      - page: 2
        x0: 85.03
        top: 292.60
        x1: 517.71
        bottom: 304.60
---

# ¿Qué es un agente? La pregunta que importa

<!-- okf-source-span: u-4 -->
Antes de definir qué es un agente, vale la pena entender por qué necesitamos el concepto.

<!-- okf-source-span: u-5 -->
Un modelo de lenguaje grande (LLM), por sí mismo, es un sistema de procesamiento de texto. Recibe una entrada, produce una salida y no retiene estado entre interacciones. Toda su utilidad depende de la calidad de la instrucción que recibe y de la información que contiene su ventana de contexto en ese instante. Si la tarea requiere consultar una base de datos, verificar un resultado intermedio, invocar una API externa o recordar algo de una sesión anterior, el modelo por sí solo no puede hacerlo. Es un motor de razonamiento lingüístico sin manos, sin memoria y sin capacidad de actuar en el mundo.

<!-- okf-source-span: u-6 -->
Considere un escenario concreto. Un cliente escribe al soporte de una empresa de logística para reportar que le cobraron un envío duplicado. Un chatbot convencional basado en un LLM puede redactar una respuesta empática y sugerir que el cliente contacte al departamento de facturación. Pero no puede verificar en el ERP si efectivamente existe un cobro duplicado, emitir la orden de reembolso en el sistema de pagos, actualizar el estado del ticket en el CRM y enviar una confirmación al cliente. Cada una de esas acciones requiere acceso a un sistema externo, lógica condicional sobre datos reales y persistencia de estado entre los pasos. El chatbot genera texto; el agente resuelve el problema.

<!-- okf-source-span: u-7 -->
**El modelo razona; el harness gobierna.**

<!-- okf-source-span: u-8 -->
El paso del razonamiento a la acción no lo realiza el modelo por sí solo. El modelo puede interpretar el objetivo, analizar la información disponible y proponer una acción, pero necesita una arquitectura que determine si esa propuesta puede ejecutarse y bajo qué condiciones. Un agente confiable surge cuando el harness transforma el razonamiento probabilístico del modelo en acciones controladas, verificables y limitadas por diseño.

<!-- okf-source-span: u-9 -->
La diferencia no es de grado sino de naturaleza. El chatbot responde. **El agente actúa**.


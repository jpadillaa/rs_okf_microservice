---
type: "Source Section"
unit_id: "fd8eed7b7dbc77c25be7436f2204edd9d556d30f2aa24dcc151e632ba18e5f71/contenido/seccion-002.md"
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
block_spans: "/contenido/seccion-002.spans.json"
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


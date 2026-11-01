---
type: Source Section
title: "Cinco atributos que definen a un agente"
description: "Sección 3 de p4.pdf: Cinco atributos que definen a un agente. Los atributos de un agente no residen exclusivamente en el modelo de lenguaje."
sources:
  - id: original
    resource: /references/original.pdf
generated:
  by: process:document-conversion
  at: "<generado>"
source_spans:
  - { id: seccion, source_id: original, kind: pdf_pages, from: 2, to: 5 }
  - { id: p-2, source_id: original, kind: pdf_pages, from: 2, to: 2 }
  - { id: p-2-3, source_id: original, kind: pdf_pages, from: 2, to: 3 }
  - { id: p-3, source_id: original, kind: pdf_pages, from: 3, to: 3 }
  - { id: p-4, source_id: original, kind: pdf_pages, from: 4, to: 4 }
  - { id: p-4-5, source_id: original, kind: pdf_pages, from: 4, to: 5 }
  - { id: p-5, source_id: original, kind: pdf_pages, from: 5, to: 5 }
---

# Cinco atributos que definen a un agente

<!-- okf-source-span: p-2 -->
Los atributos de un agente no residen exclusivamente en el modelo de lenguaje.

<!-- okf-source-span: p-2 -->
Emergen de la interacción entre las capacidades probabilísticas del modelo y los mecanismos de control proporcionados por el harness. El modelo interpreta, razona y propone acciones. El harness administra el estado, controla el flujo, valida las propuestas y coordina su ejecución.

<!-- okf-source-span: p-2 -->
La literatura converge en que un sistema debe exhibir cinco atributos para clasificarse como agéntico. Estos atributos no son independientes entre sí; operan como un sistema integrado donde la ausencia de cualquiera de ellos degrada la clasificación.

<!-- okf-source-span: p-2 -->
## 1. Razona y planifica

<!-- okf-source-span: p-2 -->
El agente recibe un objetivo abstracto y no estructurado, lo descompone en pasos lógicos, y adapta su estrategia si las condiciones cambian o los resultados intermedios no son los esperados.

<!-- okf-source-span: p-2-3 -->
A nivel de implementación, esto se materializa en el bucle de razonamiento del agente. En frameworks como *LangGraph*, el bucle se estructura como un grafo de estados donde cada nodo representa una operación (invocar el LLM, ejecutar una herramienta, evaluar un resultado) y las transiciones entre nodos están gobernadas por condiciones explícitas. El LLM no "decide" el flujo completo de una vez; evalúa el estado actual, propone una acción, observa el resultado y decide el siguiente paso.

<!-- okf-source-span: p-3 -->
Cuando este atributo está ausente, el sistema no puede manejar tareas que requieran más de un paso o que presenten ambigüedad en el camino hacia la solución. Un flujo lineal que ejecuta siempre los mismos pasos en el mismo orden no es un agente, es una cadena de procesamiento.

<!-- okf-source-span: p-3 -->
## 2. Usa herramientas

<!-- okf-source-span: p-3 -->
El agente se integra dinámicamente con APIs, bases de datos y sistemas externos para leer, procesar y escribir información en tiempo real.

<!-- okf-source-span: p-3 -->
En la práctica, las herramientas son funciones con una descripción semántica que el LLM puede seleccionar según el contexto de la tarea. En *LangChain*, una herramienta se define como una función Python con un nombre, una descripción y un esquema de entrada. El LLM evalúa qué herramienta invocar, genera los argumentos de entrada y el framework ejecuta la llamada. El resultado de la herramienta se reinyecta en el contexto del LLM para que este decida el siguiente paso.

<!-- okf-source-span: p-3 -->
El protocolo Model Context Protocol (MCP), originalmente desarrollado por *Anthropic* y adoptado como estándar de la industria, formaliza esta integración. MCP permite que un agente descubra, se autentique y utilice herramientas de sistemas dispares mediante una interfaz estandarizada, sin requerir conectores API personalizados para cada aplicación.

<!-- okf-source-span: p-3 -->
Cuando este atributo está ausente, el sistema está confinado a lo que sabe a partir de su entrenamiento y su ventana de contexto. No puede verificar hechos contra datos actuales, no puede ejecutar acciones en el mundo y no puede interactuar con la infraestructura corporativa.

<!-- okf-source-span: p-3 -->
## 3. Recuerda: mantiene una memoria persistente

<!-- okf-source-span: p-3 -->
El agente mantiene contexto entre interacciones, aprende de experiencias previas y preserva el estado de la tarea a través de reinicios del sistema.

<!-- okf-source-span: p-4 -->
La arquitectura de memoria se divide en tres niveles.

<!-- okf-source-span: p-4 -->
1. La memoria a corto plazo es el contexto de la conversación actual, limitado por la ventana del modelo.
2. La memoria a largo plazo permite al agente almacenar y recuperar información de interacciones pasadas, típicamente implementada mediante bases de datos vectoriales donde los embeddings de las interacciones previas se almacenan y se recuperan por similitud semántica.
3. La memoria persistente garantiza que el estado de una tarea en curso sobreviva a los reinicios del sistema, implementada como estado serializado en una base de datos relacional o un almacén de clave-valor.

<!-- okf-source-span: p-4 -->
En LangGraph, la persistencia se implementa mediante checkpointers que almacenan snapshots del estado del grafo después de cada nodo. Si el proceso se interrumpe, el agente puede reanudar desde el último checkpoint sin perder progreso.

<!-- okf-source-span: p-4 -->
Cuando este atributo está ausente, el agente comienza cada interacción desde cero. No puede manejar tareas que se extiendan a lo largo de horas o días, no puede aprender de errores previos y no puede mantener coherencia en conversaciones prolongadas.

<!-- okf-source-span: p-4 -->
## 4. Verifica, corrige y reintenta: ciclos de retroalimentación autónomos

<!-- okf-source-span: p-4 -->
El agente evalúa el resultado empírico de cada acción contra el objetivo original, itera su comportamiento y determina cuándo ha completado la tarea o cuándo necesita intervención humana. Este atributo es lo que distingue a un agente de un pipeline de un solo paso. El agente no asume que la primera respuesta es correcta; ***verifica, corrige y reintenta***.

<!-- okf-source-span: p-4 -->
En *LangGraph*, esto se implementa como aristas condicionales en el grafo: después de ejecutar una acción, un nodo de evaluación decide si el resultado es satisfactorio, si necesita un paso adicional o si debe escalar a un humano.

<!-- okf-source-span: p-4-5 -->
El concepto de umbrales de confianza es central. El agente no opera con certeza absoluta; opera con probabilidades. Cuando la confianza en su resultado cae por debajo de un umbral definido por el diseñador, el agente debe detenerse y solicitar intervención, no inventar una respuesta plausible.

<!-- okf-source-span: p-5 -->
Cuando este atributo está ausente, el sistema ejecuta una sola pasada sin verificación. Los errores se propagan sin corrección y la calidad del resultado depende enteramente de la primera respuesta del modelo.

<!-- okf-source-span: p-5 -->
## 5. Agencia condicionada

<!-- okf-source-span: p-5 -->
El agente actúa de forma independiente pero dentro de límites estrictos de permisos, políticas de cumplimiento, seguridad y presupuestos computacionales definidos por la organización.

<!-- okf-source-span: p-5 -->
Este es el atributo más contraintuitivo para los ingenieros que provienen de la automatización tradicional. La autonomía del agente no es un interruptor binario (autónomo o no autónomo); es un espectro configurable donde el diseñador define explícitamente qué decisiones puede tomar el agente, qué herramientas puede invocar, qué volumen de datos puede procesar y bajo qué condiciones debe detenerse y consultar a un humano.

<!-- okf-source-span: p-5 -->
En la implementación, esto se traduce en guardrails, funciones de validación que se ejecutan antes o después de cada acción del agente. Un guardrail de entrada puede verificar que la solicitud del usuario no contenga instrucciones maliciosas (inyección de prompt). Un guardrail de salida puede verificar que la respuesta del agente no incluya información sensible. Un guardrail de herramientas puede restringir qué APIs puede invocar el agente según el rol del usuario autenticado.

<!-- okf-source-span: p-5 -->
Cuando este atributo está ausente, el agente opera sin restricciones. Si el modelo es engañado, puede ejecutar acciones destructivas, acceder a datos no autorizados o exfiltrar información corporativa. La ausencia de agencia condicionada es la causa raíz de la mayoría de los incidentes de seguridad documentados en 2025 y 2026.


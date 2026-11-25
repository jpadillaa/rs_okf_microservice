---
type: "Source Section"
unit_id: "2752b1d7e68d7ceb4acfcd2de50485928778db9947d49ef7d7c2c9aab31376b9/contenido/seccion-008.md"
parent: "documento.md"
previous: "contenido/seccion-007.md"
next: "contenido/seccion-009.md"
title: "Arquitectura interna del harness"
description: "Sección 8 de d3.docx: Arquitectura interna del harness. Una forma útil de describir la arquitectura interna del harness es representarla como una estructura de seis componentes. Esta descomposición no debe…"
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
    from: 61
    to: 96
block_spans: "/contenido/seccion-008.spans.json"
---

# Arquitectura interna del harness

<!-- okf-source-span: b-62 -->
Una forma útil de describir la arquitectura interna del harness es representarla como una estructura de seis componentes. Esta descomposición no debe entenderse como una especificación universal, sino como un modelo analítico para identificar responsabilidades.

## 1. Orquestador y bucle de ejecución

<!-- okf-source-span: b-64 -->
El orquestador administra el bucle de observación, razonamiento y acción. Mantiene el estado actual, selecciona la siguiente operación y aplica las condiciones de transición y terminación. Puede establecer un número máximo de pasos, detectar estados repetidos, detener ejecuciones que no muestran progreso o escalar una tarea cuando la confianza cae por debajo de un umbral.

<!-- okf-source-span: b-65 -->
Cuando este componente está ausente o mal diseñado, el modelo adquiere control implícito sobre el flujo. El sistema puede entrar en bucles no convergentes, repetir acciones innecesarias o continuar operando después de alcanzar una condición de fallo.

## 2. Registro y ejecución de herramientas

<!-- okf-source-span: b-67 -->
El registro de herramientas mantiene un catálogo tipado y validado de las capacidades disponibles. Cada herramienta debe incluir un nombre, una descripción semántica, un esquema de entrada y un esquema de salida.

<!-- okf-source-span: b-68 -->
El modelo utiliza la descripción para proponer qué herramienta debería invocarse. El harness valida la selección, verifica los argumentos y determina si la identidad autenticada posee autorización para ejecutar la acción.

<!-- okf-source-span: b-69 -->
La ejecución material puede realizarse dentro del proceso principal, en un servicio externo, en un sandbox o en un runtime especializado. Esta separación permite que el catálogo lógico de herramientas permanezca independiente de su infraestructura de ejecución.

<!-- okf-source-span: b-70 -->
Cuando este componente está ausente, las herramientas se convierten en funciones expuestas sin control sistemático. El modelo puede generar parámetros inválidos, utilizar una herramienta fuera de contexto o intentar ejecutar una operación que excede sus permisos.

## 3. Gestor de contexto

<!-- okf-source-span: b-72 -->
El gestor de contexto determina qué información ingresa a la ventana del modelo en cada paso. Su responsabilidad no consiste simplemente en almacenar mensajes, sino en seleccionar, organizar, resumir, compactar y priorizar información.

<!-- okf-source-span: b-73 -->
Este componente puede integrar el historial de mensajes, el estado actual de la tarea, los resultados de herramientas, instrucciones operativas, políticas, documentos recuperados e información recuperada desde la memoria a largo plazo.

<!-- okf-source-span: b-74 -->
El gestor de contexto debe diferenciar entre información disponible y contexto efectivo. Un sistema puede almacenar grandes volúmenes de información, pero solo una fracción puede incorporarse a la ventana del modelo en cada invocación.

<!-- okf-source-span: b-75 -->
Cuando este componente está ausente, el historial crece sin control, aumenta el costo de ejecución y la información relevante puede quedar desplazada por mensajes secundarios. El agente también puede perder coherencia cuando un resumen elimina decisiones importantes o cuando se recupera información semánticamente similar pero operacionalmente irrelevante.

## 4. Memoria persistente y checkpointing

<!-- okf-source-span: b-77 -->
La memoria persistente garantiza que el estado de una tarea en curso sobreviva a interrupciones, reinicios o pausas prolongadas. Conserva las variables necesarias para reconstruir la ejecución y continuar desde un punto previamente alcanzado.

<!-- okf-source-span: b-78 -->
El **almacén de estado** proporciona la infraestructura donde se conserva esta información. El **checkpointer** es el mecanismo que captura y restaura snapshots del estado. El almacén determina dónde se guardan los datos; el checkpointer determina cuándo se registra el estado y cómo se recupera.

<!-- okf-source-span: b-79 -->
En LangGraph, un checkpoint puede almacenar la lista de mensajes, los resultados intermedios, la posición actual dentro del grafo y las variables asociadas a la tarea. Si el proceso se interrumpe, la ejecución puede reanudarse desde el último estado válido.

<!-- okf-source-span: b-80 -->
La memoria persistente no debe confundirse con la memoria a largo plazo. La primera preserva la continuidad de una ejecución. La segunda almacena información que puede recuperarse y reutilizarse en otras sesiones o tareas. Ambas pueden utilizar mecanismos de almacenamiento persistente, pero cumplen responsabilidades diferentes.

<!-- okf-source-span: b-81 -->
Cuando la memoria persistente está ausente, cualquier interrupción obliga al agente a comenzar de nuevo. Esto incrementa los costos, puede duplicar acciones externas y dificulta la ejecución de tareas que se extienden durante horas o días.

## 5. Lifecycle hooks, guardrails y control

<!-- okf-source-span: b-83 -->
Los *lifecycle hooks*, son puntos de intercepción ubicados antes o después de operaciones relevantes. Permiten ejecutar lógica adicional sin modificar directamente el componente principal.

<!-- okf-source-span: b-84 -->
Un hook puede utilizarse para registrar una traza, validar un argumento, comprobar permisos, aplicar una política de negocio, modificar el contexto o medir el consumo de recursos. El hook es el punto de extensión; la lógica que se ejecuta en él puede cumplir funciones de seguridad, observabilidad o transformación.

<!-- okf-source-span: b-85 -->
Un caso especialmente relevante es la activación de un mecanismo de *human-in-the-loop* (HITL). Considere un agente autorizado para preparar una transferencia bancaria, pero no para ejecutarla sin aprobación cuando el monto supera un límite determinado. Un *lifecycle hook* previo a la invocación de la herramienta inspecciona los argumentos y detecta que la operación requiere supervisión. El mecanismo HITL interrumpe entonces la ejecución, conserva el estado mediante checkpointing y espera la decisión humana. La persona puede aprobar, modificar o rechazar la acción; posteriormente, el orquestador reanuda el flujo desde el estado conservado o termina la tarea. El *lifecycle hook* detecta la condición que requiere supervisión, mientras que HITL implementa la pausa y la intervención humana.

<!-- okf-source-span: b-86 -->
Los guardrails constituyen la categoría general de controles que restringen el comportamiento del sistema. Pueden aplicarse sobre las entradas del usuario, las salidas del modelo, los argumentos de las herramientas o las acciones realizadas en el entorno.

<!-- okf-source-span: b-87 -->
Un **circuit breaker**, es un mecanismo específico dentro de esta capa. Interrumpe o degrada la ejecución cuando una señal supera un umbral definido. Las señales pueden incluir errores consecutivos, número de reintentos, latencia, consumo de tokens o ausencia de progreso.

<!-- okf-source-span: b-88 -->
Cuando estos controles están ausentes, el sistema depende de que el modelo interprete correctamente sus propias restricciones. Una inyección de prompt, una salida mal estructurada o una evaluación incorrecta pueden convertirse directamente en acciones no autorizadas.

## 6. Observabilidad y evaluación

<!-- okf-source-span: b-90 -->
La capa de observabilidad y evaluación instrumenta la ejecución completa del agente. Registra las invocaciones al modelo, las llamadas a herramientas, las transiciones del grafo, los cambios de estado, los errores, la latencia y el consumo de recursos.

<!-- okf-source-span: b-91 -->
La observabilidad permite reconstruir qué ocurrió durante una trayectoria. La evaluación determina si el comportamiento y el resultado cumplen los criterios definidos.

<!-- okf-source-span: b-92 -->
La evaluación puede realizarse después de la ejecución para comparar versiones del sistema, pero también puede operar dentro del flujo. Un evaluador puede decidir si un resultado es aceptable, si debe ejecutarse un nuevo intento o si la tarea requiere intervención humana.

<!-- okf-source-span: b-93 -->
El evaluador no tiene que ser necesariamente un segundo agente. Puede implementarse mediante reglas deterministas, esquemas de validación, pruebas automatizadas, compiladores, linters, otro modelo, revisión humana o una combinación de estos mecanismos.

<!-- okf-source-span: b-94 -->
Cuando esta capa está ausente, los errores resultan difíciles de reproducir y el equipo no puede distinguir si una falla provino del modelo, del contexto, de una herramienta, de la memoria persistente o del orquestador.

<!-- okf-source-span: b-95 -->
![Imagen 5](../assets/imagen-005.png)

<!-- okf-source-span: b-96 -->
**Figura 4.** Arquitectura interna de un harness.

---
type: "Source Section"
unit_id: "b05d6f617629f487828a62a96fba6edfa759140232f5d438344e470f68bd9d06/contenido/seccion-003.md"
parent: "documento.md"
previous: "contenido/seccion-002.md"
next: "contenido/seccion-004.md"
title: "Paso No. 1 – Estimación de infraestructura y Capex para el despliegue on-premise de modelos de lenguaje de código abierto"
description: "Sección 3 de d2.docx: Paso No. 1 – Estimación de infraestructura y Capex para el despliegue on-premise de modelos de lenguaje de código abierto. Determinar la capacidad y los recursos tecnológicos requeridos para implementar y operar, en infraestructura local, el modelo de código abierto asignado al…"
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
    from: 15
    to: 23
block_spans: "/contenido/seccion-003.spans.json"
---

# Paso No. 1 – Estimación de infraestructura y Capex para el despliegue on-premise de modelos de lenguaje de código abierto

<!-- okf-source-span: b-16 -->
Determinar la capacidad y los recursos tecnológicos requeridos para implementar y operar, en infraestructura local, el modelo de código abierto asignado al equipo de trabajo, mediante una evaluación detallada de los componentes de hardware, software y soporte necesarios para garantizar un desempeño eficiente.

<!-- okf-source-span: b-17 -->
El análisis debe considerar una arquitectura de dos niveles, adecuada para un contexto universitario como el de la Universidad de los Andes. En este esquema, se evalúa un modelo de mayor capacidad como servicio central desplegado sobre un nodo de clúster, complementado por un modelo más liviano que pueda ejecutarse en laboratorios o estaciones individuales.

<!-- okf-source-span: b-18 -->
Esta propuesta integra un **patrón de enrutamiento** que utiliza **Gemma 4 31B Dense** como modelo central y **Gemma 4 26B A4B** como modelo satelital para optimizar la carga de trabajo. En esta arquitectura, una **capa de clasificación** evalúa la complejidad del prompt y asigna las **tareas estándar**, tales como consultas RAG o extracción de entidades, al modelo **26B A4B** en favor de su baja latencia y eficiencia MoE. Las **tareas críticas** que exigen razonamiento lógico complejo o depuración de código se reservan para el modelo **31B Dense** en su rol de respaldo de alta capacidad.

<!-- okf-source-span: b-19 -->
Al pertenecer a la misma familia tecnológica bajo licencia **Apache 2.0**, este par garantiza una tokenización idéntica y una documentación unificada que facilita el despliegue en infraestructura cloud. Esta simetría técnica asegura la consistencia en el espacio de embeddings y minimiza la deriva semántica durante la transferencia de estados de memoria en arquitecturas de agentes multi-nivel.

<!-- okf-source-span: b-20 -->
En la capa de serving e integración, se recomienda considerar herramientas como vLLM o SGLang, junto con un API gateway como LiteLLM para exponer una interfaz compatible con OpenAI.

<!-- okf-source-span: b-21 -->
En este ejercicio se debe estimar el costo de la infraestructura de TI necesaria, incluyendo servidores de alto rendimiento con GPU. Asimismo, se debe proyectar el Capex anual de la inversión, incorporando los costos de adquisición, instalación, mantenimiento y escalabilidad de la solución, con el fin de obtener una visión integral del presupuesto requerido para una implementación exitosa en la Universidad de los Andes.

<!-- okf-source-span: b-22 -->
El análisis debe contemplar la inclusión de servidores y almacenamiento de alta velocidad. También debe determinar el número de GPU requeridas, por ejemplo NVIDIA A100 o H100, así como la memoria VRAM y el ancho de banda de red, en función del tamaño del modelo y de la carga esperada, incluyendo usuarios concurrentes y tokens generados por segundo. De igual forma, debe calcular el consumo energético de esta infraestructura, a partir de supuestos razonables de operación continua y de la tarifa industrial colombiana.

<!-- okf-source-span: b-23 -->
No es necesario estimar sistemas de almacenamiento adicionales, redes de alta velocidad ni otros componentes de soporte técnico, dado que la Universidad cuenta con un centro de datos sólido en estos aspectos.

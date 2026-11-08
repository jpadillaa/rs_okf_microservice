---
type: "Source Section"
unit_id: "a42ed4f41a1be1893de014cf90d674b7ca5c484f6af5abdd20e82387b64c29ce/contenido/seccion-002.md"
title: "Introducción - ¿Por qué necesitamos el concepto de harness?"
description: "Sección 2 de d3.docx: Introducción - ¿Por qué necesitamos el concepto de harness?. Durante los primeros años de los agentes basados en modelos de lenguaje, buena parte de la industria asumió que la calidad de un agente dependía principalmente…"
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
    from: 3
    to: 11
  - id: "b-4"
    source_id: "original"
    kind: "docx_blocks"
    from: 4
    to: 4
  - id: "b-5"
    source_id: "original"
    kind: "docx_blocks"
    from: 5
    to: 5
  - id: "b-6"
    source_id: "original"
    kind: "docx_blocks"
    from: 6
    to: 6
  - id: "b-7"
    source_id: "original"
    kind: "docx_blocks"
    from: 7
    to: 7
  - id: "b-8"
    source_id: "original"
    kind: "docx_blocks"
    from: 8
    to: 8
  - id: "b-9"
    source_id: "original"
    kind: "docx_blocks"
    from: 9
    to: 9
  - id: "b-10"
    source_id: "original"
    kind: "docx_blocks"
    from: 10
    to: 10
  - id: "b-11"
    source_id: "original"
    kind: "docx_blocks"
    from: 11
    to: 11
---

# Introducción - ¿Por qué necesitamos el concepto de harness?

<!-- okf-source-span: b-4 -->
Durante los primeros años de los agentes basados en modelos de lenguaje, buena parte de la industria asumió que la calidad de un agente dependía principalmente de la capacidad del LLM. La expectativa era directa: ***un modelo suficientemente competente, acompañado de un prompt elaborado, debería producir comportamientos confiables***.

<!-- okf-source-span: b-5 -->
La experiencia práctica demostró que esa premisa era incompleta. Los agentes podían interpretar correctamente un objetivo y, aun así, quedar atrapados en ciclos no convergentes, inventar parámetros al invocar una herramienta, perder información relevante cuando el historial excedía la ventana de contexto o ejecutar una acción que el usuario no estaba autorizado a realizar. En estos casos, aumentar la capacidad del modelo no corregía necesariamente el problema, porque la falla no se encontraba en el razonamiento semántico, sino en la infraestructura encargada de convertir ese razonamiento en una ejecución controlada.

<!-- okf-source-span: b-6 -->
Considere un agente de desarrollo de software que recibe la instrucción de corregir un error en un repositorio. El modelo puede identificar correctamente la causa del problema y proponer una modificación apropiada. Sin embargo, el sistema todavía debe determinar qué archivos puede leer, qué rutas puede modificar, cómo ejecutar las pruebas, cuántos intentos están permitidos, qué hacer cuando la compilación falla y bajo qué condiciones debe solicitar intervención humana. También debe impedir que el agente acceda a credenciales, modifique archivos fuera del repositorio o ejecute comandos destructivos.

<!-- okf-source-span: b-7 -->
Estas responsabilidades no pertenecen al modelo de lenguaje. Corresponden al **harness**.

<!-- okf-source-span: b-8 -->
tenerse o solicitar intervención.

<!-- okf-source-span: b-9 -->
![Imagen 2](../assets/imagen-002.png)

<!-- okf-source-span: b-10 -->
**Figura 1.** Del razonamiento probabilístico a la ejecución gobernada. El harness como arquitectura de control entre el modelo de lenguaje y el entorno.

<!-- okf-source-span: b-11 -->
El modelo propone. El harness controla la ejecución.

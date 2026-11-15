---
type: "Source Section"
unit_id: "654b7977759cc5cb02cd02fc8433d82b6fb1c1fbff8a95f4e6d72c9ca8ed542d/contenido/seccion-004.md"
parent: "documento.md"
previous: "contenido/seccion-003.md"
next: "contenido/seccion-005.md"
title: "Lo que un agente NO es"
description: "Sección 4 de p4.pdf: Lo que un agente NO es. La industria ha generado confusión significativa al etiquetar como \"agentes\" a sistemas que no lo son. Gartner denomina a esta práctica \"agent-washing\" y…"
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
    from: 5
    to: 6
block_spans: "/contenido/seccion-004.spans.json"
---

# Lo que un agente NO es

<!-- okf-source-span: u-42 -->
La industria ha generado confusión significativa al etiquetar como "agentes" a sistemas que no lo son. Gartner denomina a esta práctica "agent-washing" y estima que más del

<!-- okf-source-span: u-43 -->
40% de los proyectos agénticos serán cancelados antes de finales de 2027 por expectativas mal calibradas. Para un ingeniero que va a construir estos sistemas, las distinciones son operativas, no semánticas.

<!-- okf-source-span: u-44-47 -->
- **Un agente no es un chatbot**. Un chatbot procesa una entrada y produce una salida textual. No invoca herramientas, no mantiene estado entre sesiones y no puede ejecutar acciones en sistemas externos. Un chatbot conectado a una base de datos de FAQ sigue siendo un chatbot, no un agente.
- **Un agente no es un pipeline de RPA**. La automatización robótica de procesos (RPA: Robotic Process Automation ) opera bajo lógica condicional rígida: ***if-then-else***. Cuando las condiciones se desvían de los parámetros anticipados, el pipeline falla silenciosamente o se detiene. Un agente razona sobre la desviación y busca una ruta alternativa hacia el objetivo. La diferencia es entre seguir instrucciones y resolver problemas.
- **Un agente no es una cadena sin memoria ni herramientas**. Una cadena que conecta un prompt con un modelo y produce una respuesta es un pipeline de procesamiento de texto. Puede ser útil, pero no es agéntico. Para que sea agéntico necesita, como mínimo, la capacidad de invocar herramientas, mantener estado y evaluar sus propios resultados.
- **Un agente no es un copiloto con intervención humana obligatoria en cada paso**. La distinción radica en la autonomía, el agente ejecuta secuencias de acciones sin requerir aprobación paso a paso, aunque puede escalar a un humano cuando su confianza es insuficiente.


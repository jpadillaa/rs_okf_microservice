---
type: "Slide"
title: "3. El costo depende de qué se procesa"
description: "Diapositiva 8 de s1.pptx: 3. El costo depende de qué se procesa. USD por 1 millón de tokens · API estándar"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-8"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 8
    slide_id: 263
    display_label: "08"
title_inferred: true
kicker: "¿POR QUÉ CONSIDERAR RAG?"
---

# 3. El costo depende de qué se procesa

> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

- USD por 1 millón de tokens · API estándar
- MODELO DE EJEMPLO
- ENTRADA SIN CACHÉ
- SALIDA FACTURABLE
- CONDICIÓN
- Gemini 3.8 Flash
- US$0,75
- US$3,75
- Tarifas hasta 31-dic-2026
- GPT-6.1 Sol
- US$2,00
- US$10,00
- Hasta 272.000 tokens de entrada
- Claude Opus 5.5
- US$4,00
- US$20,00
- Tarifa estándar
- EJEMPLO HIPOTÉTICO · GPT-6.1 SOL · SOLO ENTRADA
- La salida se factura por separado
- 100\.000 tokens de entrada
- US$0,20
- Menor volumen procesado
- 10\.000 tokens de entrada
- US$0,02
- COSTO TOTAL
- Preparación e indexación
- \+
- Recuperación y generación
- \+
- Operación
- Comparar costo total, calidad y latencia
- Consulta: 08-oct-2026 · Ejemplo de cálculo hipotético; no es una medición de RAG

## Notas del presentador

GUÍA DOCENTE

Explique que las APIs facturan consumos y que entrada y salida tienen tarifas diferentes. La tabla utiliza modelos actuales para ilustrar esa distinción, con precios estándar de los proveedores, entrada sin caché y condiciones explícitas. Las tarifas no equivalen a un juicio de calidad y los tokenizadores pueden producir consumos distintos para un mismo texto. Los modelos de razonamiento también pueden generar tokens facturables que no coinciden con la extensión visible de la respuesta.

Muestre un cálculo hipotético del cargo de entrada con GPT-6.1 Sol: 100.000 tokens a US$2 por millón cuestan US$0,20; 10.000 cuestan US$0,02. Esa diferencia permite explicar el efecto del volumen procesado, sin demostrar un ahorro total de RAG ni conservar automáticamente la calidad. Recuperar introduce costos de preparación, almacenamiento y consulta. La decisión requiere medir consumo, calidad y latencia con la configuración elegida, incluyendo caché y otras condiciones comerciales cuando correspondan.

CONDUCCIÓN · 08:00–10:00 · 2 min

00:00–00:35 · Estado inicial: tabla y condiciones.

00:35–01:10 · Paso 1: tarjetas de cálculo; solo representan entrada.

01:10–01:40 · Paso 2: componentes de costo total.

01:40–02:00 · Paso 3: comparación con calidad y latencia.

CONDICIONES VERIFICADAS

Gemini 3.8 Flash: Google anuncia US$1,50 de entrada y US$7,50 de salida a partir del 1 de enero de 2027; el slide usa las tarifas vigentes en la fecha de consulta. GPT-6.1 Sol: una entrada superior a 272.000 tokens aplica al total de la solicitud el doble de tarifa de entrada y 1,5 veces la de salida; la tabla usa el tramo inferior. Caché, lotes, herramientas, residencia regional y otras modalidades se verifican por separado. No es una comparación de calidad.

FUENTES

Gemini Developer API pricing: https://ai.google.dev/gemini-api/docs/pricing

GPT-6.1 Sol: https://developers.openai.com/api/docs/models/gpt-6.1-sol

Claude Opus 5.5: https://platform.claude.com/docs/en/models/opus-5-5/overview

Thinking tokens (Claude): https://platform.claude.com/docs/en/build-with-claude/context-windows

## Vista visual

![Vista de la diapositiva 8](../assets/slide-008.png)

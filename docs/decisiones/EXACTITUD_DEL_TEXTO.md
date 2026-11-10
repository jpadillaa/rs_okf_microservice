# Exactitud del texto PDF

**10 de noviembre de 2026.** La [cobertura](COBERTURA_INDEPENDIENTE.md) compara bolsas de palabras: dice si una palabra llega al bundle, no si llega en su sitio. Un texto con las letras de dos líneas intercaladas, una tabla que parte una frase entre filas o un índice cuyas entradas se funden en una celda pueden conservar todas sus palabras y aprobarla. La completitud y la exactitud son los requisitos máximos de calidad, así que este documento fija cómo se mide la exactitud: el orden del texto y que no se mezcle.

## Métricas

`tools/exactitud.py` compara, página por página, la secuencia de palabras de una referencia con la de los bloques del bundle cuyo localizador (`pdf_pages`, del frontmatter o del archivo `block_spans`) cubre esa página. Los bloques van en el orden del Markdown, con el título de su concepto en la página donde empieza y el del documento (`titulo`). Las palabras se normalizan como en la cobertura (NFKC, casefold, letras y cifras). No cuentan como texto del original las notas marcadas con `<!-- okf-generated -->`, los rótulos de imágenes, el rótulo «Texto girado:», los marcadores ni los destinos de los enlaces.

| Métrica | Qué mide | Qué la baja |
| --- | --- | --- |
| Completitud | Palabras de la referencia presentes en la página del bundle. | Texto perdido. |
| Orden | De las palabras presentes, las que están en el orden de la referencia (subsecuencia común más larga). | Un bloque o una columna fuera de su sitio. |
| Orden interno | Lo mismo dentro de cada bloque, frente a la referencia de las páginas que ocupa, sin importar dónde está el bloque. Se cuenta en la primera página del bloque. | Un párrafo, una celda o una línea que mezcla texto de sitios distintos. |
| Contigüidad | Pares de palabras consecutivas de la referencia que el bundle conserva juntos. | Líneas o celdas intercaladas. |

La subsecuencia común más larga se calcula por bits (Hyyrö, 2004) en Python y por programación dinámica en Rust; ambas dan las mismas cifras en las muestras.

## Referencias

**Exactas.** `examples/pdf/exactitud/<grupo>/<nombre>.json` guarda el texto de cada página en orden de lectura: `{"document": ..., "source": ..., "pages": {"1": "texto…"}}`. Las de las muestras públicas las escribe `examples/pdf/create_samples.py` al dibujar cada PDF, así que son correctas por construcción y no cambian los bytes de los PDF. Omiten encabezados, pies y números de página, que el bundle registra una vez en `documento.md`. Colocan el texto girado al final de su página y la nota al pie detrás del párrafo que la llama, donde los pone la especificación. Las palabras partidas al final de línea se reúnen con la regla de la especificación. `escaneado_ocr.pdf` es la imagen de `informe_tecnico.pdf` y comparte su referencia, de modo que mide también el orden del texto que devuelve el OCR.

**Aproximadas.** Sin referencia exacta, la herramienta usa el texto de pdfium. Antes de comparar:
- reúne las palabras que pdfium parte al final de línea (con guion, guion suave o el U+0002 con que marca el guion que inserta);
- retira de la referencia las palabras ausentes que el bundle registra en `documento.md`, empezando por los bordes de la página;
- da por presente una palabra ausente de cuatro letras o más que aparece dentro del texto del bundle sin espacios.

pdfium sigue el flujo de contenido: deja los rótulos de una figura al final de la página y, en exportaciones como las de Canva, separa letras. Por eso su **orden** global no prueba un error. El **orden interno** y la **contigüidad**, que no dependen de dónde está cada bloque, sí son significativos.

**Revisadas a mano.** Para un original privado se puede escribir una referencia exacta de algunas páginas, con el mismo formato, y pasarla con `--reference`; se guarda fuera del repositorio, junto al original en `datos_de_prueba/`. Solo se miden sus páginas.

## Uso

```bash
uv run tools/exactitud.py examples/pdf/muestras/*.pdf
```

```bash
uv run tools/exactitud.py --bundle bundle.zip --reference revisada.json --json salida.json --worst 10 original.pdf
```

Por defecto convierte con `target/release/okf-microservice` (`--binary` cambia el ejecutable) y la configuración del entorno: con `OKF_OCR_ENABLED=false` la muestra escaneada no tiene texto. Lista las peores páginas con el primer tramo en que el bundle se aparta de la referencia; `--json` guarda las cifras de cada página.

## Comprobación en las pruebas

`src/exactness.rs` repite el cálculo con las referencias exactas y lo compara con `examples/exactitud-thresholds.json`: por documento y métrica, la cifra medida y un mínimo medio punto por debajo, que se actualizan a mano como los de la cobertura. Regenerar los golden no los cambia. La muestra escaneada se convierte con el OCR si están los modelos (la CI los exige). Otra prueba comprueba que el cálculo detecta texto perdido, bloques reordenados y líneas intercaladas, y que ignora notas y rótulos del conversor.

## Línea base (perfil 1.17.0)

| Muestra pública (referencia exacta) | Completitud | Orden | Orden interno | Contigüidad |
| --- | ---: | ---: | ---: | ---: |
| `articulo_columnas.pdf` | 100 | 100 | 100 | 99,80 |
| `breve.pdf` | 100 | 100 | 100 | 100 |
| `escaneado_mixto.pdf` | 100 | 100 | 100 | 100 |
| `escaneado_ocr.pdf`, con OCR | 100 | 100 | 100 | 100 |
| `informe_tecnico.pdf` | 100 | 100 | 100 | 100 |

En el artículo, la contigüidad pierde un par de palabras en el salto de la página 1 a la 2: la especificación coloca las afiliaciones detrás del párrafo que continúa en la página siguiente.

Con referencia aproximada (pdfium), antes (perfil 1.16.0) y después de los cambios de calidad del perfil 1.17.0:

| Original privado | Completitud | Orden interno | Contigüidad |
| --- | ---: | ---: | ---: |
| Artículo de 9 páginas | 99,22 → 99,22 | 99,17 → 99,17 | 94,05 → 93,87 |
| Presentación de 53 páginas | 99,41 → 99,41 | 97,94 → 97,98 | 89,96 → 90,15 |
| Cuaderno de 65 páginas | 99,94 → 99,94 | 94,19 → 94,19 | 91,31 → 91,31 |
| PDF de 346 páginas | 99,58 → 99,59 | 98,75 → 99,05 | 95,09 → 95,31 |
| 575 páginas con OCR previo | 99,93 → 99,94 | 99,74 → 99,74 | 99,24 → 99,25 |

En el PDF de 346 páginas mejoran 26 páginas: la contraportada con citas intercaladas pasa de 85 a 95 de contigüidad, y las páginas con rótulos de diagramas de 84 y 89 a 100 de orden interno. La métrica destapó además una regresión de esos mismos cambios: en el índice del libro, la regla que une filas de tabla que continúan su texto fundía entradas en minúscula («acknowledgments», «about this book») con sus páginas en romanos. Se corrigió antes de publicar el perfil: una fila con un número, en cifras o en romanos, o bajo uno, es una entrada propia. Otra página que parecía empeorar era una mejora mal atribuida: una lista ahora unida con su continuación se cuenta en su primera página.

## Límites

- Las referencias exactas cubren cuatro maquetaciones sintéticas. Los originales reales solo tienen referencia aproximada, hasta que se revisen a mano sus páginas difíciles.
- La métrica mide el texto, no su marcado: no comprueba que un encabezado sea encabezado, que una tabla tenga las celdas correctas o que un bloque de código conserve su sangría.
- Una referencia fija una convención de lectura: notas al pie detrás de su párrafo, texto girado al final de su página. Si la especificación cambia una convención, sus referencias cambian con ella.

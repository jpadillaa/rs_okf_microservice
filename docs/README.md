# Documentación del microservicio OKF

Este directorio es el punto de entrada para implementar el conversor de PDF, DOCX, XLSX y PPTX a bundles OKF v0.2. Los documentos describen el **resultado**, el **comportamiento del servicio** y las **herramientas elegidas**. Léelos en el orden de la tabla antes de escribir código.

## Mapa y orden de lectura

| Orden | Ubicación en el repositorio | Documento actual | Qué resuelve |
| --- | --- | --- | --- |
| 1 | `docs/especificacion/ESPECIFICACION_OKF_DOCUMENTOS.md` | [Especificación OKF documental](https://chatgpt.com/library?file_id=libfile_c68ef3f3f8108191b414a8a632263394&account_id=personal) | Contrato del bundle: archivos, frontmatter, recursos, procedencia, reglas por formato y validación. |
| 2 | `docs/arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md` | [Arquitectura conceptual](https://chatgpt.com/library?file_id=libfile_8c981d1e01148191bf2280b1ae1f591d&account_id=personal) | Responsabilidades, etapas de conversión, modelo intermedio y estados. |
| 3 | `docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md` | [Arquitectura técnica](https://chatgpt.com/library?file_id=libfile_791496d6b0e88191b9bf16e48563aa7a&account_id=personal) | Dos procesos, almacenamiento local, trabajo durable, reintentos, límites y publicación. |
| 3a | `docs/arquitectura/DIAGRAMA_MICROSERVICIO_OKF.md` | [Diagrama conceptual](https://chatgpt.com/library?file_id=libfile_42df649bb2d08191864d261b05adaa62&account_id=personal) | Vista del flujo descrito por las dos arquitecturas. |
| 4 | `docs/decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md` | [Stack tecnológico](https://chatgpt.com/library?file_id=libfile_861ea24f23108191ac8b630718249fe8&account_id=personal) | Rust, bibliotecas, persistencia, renderizado y pruebas necesarias antes de cerrar los adaptadores. |

Al preparar el repositorio, copia cada documento actual a la ubicación indicada sin cambiar su nombre. Así este índice y la documentación podrán mantenerse junto al código.

## Precedencia

1. La [especificación oficial de OKF v0.2](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md) y el perfil documental del proyecto rigen el bundle. El perfil añade requisitos de este conversor; no debe invalidar la especificación oficial.
2. Las arquitecturas rigen el flujo y las garantías operativas. El diagrama sirve para orientarse; si un detalle del dibujo difiere del texto, se sigue el texto.
3. El documento de stack selecciona herramientas para cumplir los contratos anteriores. Una limitación de una librería no autoriza a eliminar una referencia, un recurso o una validación exigidos por el perfil.

Si encuentras una contradicción real entre documentos, registra el caso concreto y corrige primero el documento que define ese contrato; después actualiza los documentos dependientes. No resuelvas el conflicto con una suposición silenciosa en el código.

## Para comenzar la implementación

- **Primera entrega: PPTX de extremo a extremo.** Prepara solo sus archivos de ejemplo y expectativas. Implementa recepción, worker, extracción de diapositivas, notas, recursos y vistas; genera y valida el bundle OKF. Define únicamente el modelo intermedio común que esta entrega necesite.
- Conserva `slide_index`, `slide_id` cuando exista y los demás localizadores antes de generar Markdown. Comprueba también diapositivas ocultas y sin título, vínculos, cobertura e informe.
- Cierra la entrega cuando pasen sus pruebas de aceptación y funcionen los estados `complete`, `partial` y `failed` según corresponda. Después prepara ejemplos, pruebas y adaptador de **un solo formato adicional**. Repite hasta completar PDF, DOCX y XLSX.
- Activa las dependencias por etapa según el stack. Las alternativas de bibliotecas se eligen con mediciones del formato en curso, no se instalan todas desde el inicio.

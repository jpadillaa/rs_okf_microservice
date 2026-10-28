# Documentación del microservicio OKF

Este directorio es el punto de entrada para implementar el conversor de PDF, DOCX, XLSX y PPTX a bundles OKF v0.2. Los documentos describen el **resultado**, el **comportamiento del servicio** y las **herramientas elegidas**. Léelos en el orden de la tabla antes de escribir código.

## Mapa y orden de lectura

| Orden | Ubicación en el repositorio                                      | Documento actual                                                                                   | Qué resuelve                                                                                        |
| ----- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| 1     | `docs/especificacion/ESPECIFICACION_OKF.md`                      | [Especificación OKF documental](especificacion/ESPECIFICACION_OKF.md)                              | Contrato del bundle: archivos, frontmatter, recursos, procedencia, reglas por formato y validación. |
| 2     | `docs/arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md` | [Arquitectura conceptual](arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md)              | Responsabilidades, etapas de conversión, modelo intermedio y estados.                               |
| 3     | `docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md`    | [Arquitectura técnica](arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md)                    | Dos procesos, almacenamiento local, trabajo durable, reintentos, límites y publicación.             |
| 3a    | `docs/arquitectura/DIAGRAMA_MICROSERVICIO_OKF.md`                | [Diagrama conceptual](arquitectura/DIAGRAMA_MICROSERVICIO_OKF.md)                                 | Vista del flujo descrito por las dos arquitecturas.                                                 |
| 4     | `docs/decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md`         | [Stack tecnológico](decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md)                             | Rust, bibliotecas, persistencia, renderizado y pruebas necesarias antes de cerrar los adaptadores.  |

Los documentos están guardados en las ubicaciones indicadas y se enlazan mediante rutas relativas para que funcionen dentro del repositorio y en Obsidian.

## Precedencia

1. La [especificación oficial de OKF v0.2](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md) y el perfil documental del proyecto rigen el bundle. El perfil añade requisitos de este conversor; no debe invalidar la especificación oficial.
2. Las arquitecturas rigen el flujo y las garantías operativas. El diagrama sirve para orientarse; si un detalle del dibujo difiere del texto, se sigue el texto.
3. El documento de stack selecciona herramientas para cumplir los contratos anteriores. Una limitación de una librería no autoriza a eliminar una referencia, un recurso o una validación exigidos por el perfil.

Si encuentras una contradicción real entre documentos, registra el caso concreto y corrige primero el documento que define ese contrato; después actualiza los documentos dependientes. No resuelvas el conflicto con una suposición silenciosa en el código.

## Para comenzar la implementación

- Define el modelo intermedio y el validador a partir del perfil OKF. Cada bloque citable debe conservar su localizador de origen antes de generar Markdown.
- Implementa recepción y worker conforme a los estados y garantías de la arquitectura técnica. La solicitud HTTP no espera el procesamiento.
- Sigue las elecciones iniciales del stack y sus pruebas por formato. Trata las alternativas de bibliotecas como decisiones condicionadas a resultados medidos, no como dependencias que deban instalarse todas.
- Considera terminada una conversión solo cuando el bundle sea válido, la cobertura esté declarada y el informe distinga `complete`, `partial` y `failed` según los contratos anteriores.

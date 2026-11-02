# Instrucciones para agentes

## Documentación

- Conserva el contenido y el propósito de la documentación existente al reorganizarla.
- Mantén los enlaces internos actualizados cuando cambien las rutas de los documentos.
- La documentación se escribe en español. El índice está en [`docs/README.md`](docs/README.md); todo documento nuevo se enlaza desde allí.
- La especificación, las arquitecturas y el stack describen lo que hace el código. Un cambio de comportamiento actualiza el documento que lo regula en el mismo commit.
- Todo cambio de una regla de la especificación sube la versión del perfil en `src/version.rs` y en la especificación, con su registro de cambios, en el mismo commit.
- Al cerrar un trabajo de un plan (`docs/planes/`), actualiza el [estado del proyecto](docs/estado/ESTADO_DEL_PROYECTO.md): corte, tablas afectadas y roadmap.

## Código y verificación

- Antes de cada commit, ejecuta `cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings` y `cargo test --locked`.
- `src/golden.rs` compara cada documento de `examples/` con su bundle de referencia en `examples/<formato>/esperado/`. Tras un cambio intencionado, regenera con `OKF_UPDATE_GOLDEN=1 cargo test golden` y revisa la diferencia antes de hacer commit.
- Los bundles se escriben solo a través de `src/output.rs` (frontmatter, índices, `documento.md`, informe). Ningún adaptador escribe frontmatter con plantillas de texto.
- Las variables `OKF_*` se documentan en la tabla de la arquitectura técnica, §5.
- Un cambio en un adaptador se mide también con `uv run tools/coverage.py` (ver [cobertura](docs/decisiones/COBERTURA_INDEPENDIENTE.md)). Si cambia una cifra, explícala o actualiza las listas con `--write-reference`.

## Dependencias

- `cargo deny check` debe pasar. Una excepción nueva en `deny.toml` lleva su motivo.
- Las versiones de la imagen (`Dockerfile`) están fijadas por digest y se registran en el stack.

## Datos de prueba y privacidad

- `datos_de_prueba/` contiene originales privados y no se versiona. Al repositorio solo llegan las copias reducidas de `examples/*/corpus/`, generadas con los scripts `reduce_corpus.py`.
- El recibo `p3.pdf` nunca entra al repositorio, tampoco reducido ni como lista de palabras.

## Commits

- Mensajes en inglés: un asunto en imperativo y un cuerpo que explica qué cambia y por qué. Si el trabajo pertenece a un plan, nombra su identificador (por ejemplo, «Plan 1, R4»).

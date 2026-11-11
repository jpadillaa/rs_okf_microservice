@AGENTS.md

## Contexto para Claude

- Microservicio en Rust que convierte PPTX, DOCX, XLSX y PDF en bundles OKF v0.2. Un ejecutable con los subcomandos `api`, `worker`, `validate` y `convert` (interno); cada adaptador está en `src/<formato>/`.
- Qué está hecho y qué falta: [estado del proyecto](docs/estado/ESTADO_DEL_PROYECTO.md). Trabajo planificado: [plan 1](docs/planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md) (hecho, salvo el corpus R6, en curso), [plan 2](docs/planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md) (hechos C1 a C4 y C6 a C10; C5 con infraestructura, pendiente de la 0.3.79 de `pdf_oxide` y de dos horas de fuzzing por objetivo) y [plan 3](docs/planes/PLAN_3_CONTRATO_DE_SALIDA_PARA_RAG.md) (contrato de salida para el RAG; completo: fases A, B y C).
- El perfil del bundle va por la versión 1.18.0 (`src/version.rs`); su registro de cambios está al final de la [especificación](docs/especificacion/ESPECIFICACION_OKF.md).
- El adaptador PDF tiene OCR local (`OKF_OCR_ENABLED`, activo por defecto desde el perfil 1.17.0; las pruebas y los golden convierten sin él) en `src/pdf/ocr.rs`: transcribe las páginas escaneadas y, desde el 1.18.0, el texto de las imágenes de contenido. Antes de tocarlo, lee [OCR local](docs/decisiones/OCR_LOCAL.md): sus constantes (enderezado, anchos de 64 px, tamaño por la tinta, margen derecho y presupuesto de tiempo) salen de mediciones. `tools/ocr_models.sh` descarga los modelos en `models/ocr`, y `examples/pdf/create_scans.py` regenera la muestra escaneada.
- `tools/coverage.py` y `tools/exactitud.py` (orden y mezcla del texto PDF frente a referencias exactas; ver [exactitud](docs/decisiones/EXACTITUD_DEL_TEXTO.md)) declaran sus dependencias y se ejecutan con `uv run`; los scripts de `examples/*/` indican en su cabecera los paquetes que necesitan.
- La imagen se prueba con `docker build --build-arg OKF_BUILD_COMMIT="$(git rev-parse HEAD)" -t okf-microservice:smoke .` y `tools/smoke.sh okf-microservice:smoke`; la prueba de humo termina con un PDF escaneado convertido con el OCR activo.
- `datos_de_prueba/` tiene los originales privados del corpus y un primer escaneo real (`documentoparaocr.pdf`); se usan solo para medir en local.

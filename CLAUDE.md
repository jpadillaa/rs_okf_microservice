@AGENTS.md

## Contexto para Claude

- Microservicio en Rust que convierte PPTX, DOCX, XLSX y PDF en bundles OKF v0.2. Un ejecutable con los subcomandos `api`, `worker` y `convert` (interno); cada adaptador está en `src/<formato>/`.
- Qué está hecho y qué falta: [estado del proyecto](docs/estado/ESTADO_DEL_PROYECTO.md). Trabajo planificado: [plan 1](docs/planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md) (hecho, salvo el corpus R6) y [plan 2](docs/planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md) (hechos C1 y C2).
- `tools/coverage.py` declara sus dependencias y se ejecuta con `uv run`; los scripts de `examples/*/` indican en su cabecera los paquetes que necesitan.
- La imagen se prueba con `docker build -t okf-microservice:smoke .` y `tools/smoke.sh okf-microservice:smoke`.

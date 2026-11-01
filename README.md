# Microservicio OKF

Documentación y materiales del microservicio OKF.

Consulta el índice de documentación en [`docs/README.md`](docs/README.md).

## Ejecución con Docker

```bash
docker compose up --build
```

La API queda en `http://127.0.0.1:8080` (`POST /jobs`, `GET /jobs/{id}`, `GET /jobs/{id}/bundle`) y el worker convierte en un contenedor sin red; los datos persisten en el volumen `okf-data`. Las variables `OKF_*` de la [arquitectura técnica, §5](docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf) se pasan con `environment` en `compose.yaml`.

# Compilación y empaquetado Docker

**Fecha:** 4 de noviembre de 2026. Complementa el [stack, §1](STACK_TECNOLOGICO_MICROSERVICIO_OKF.md#1-decisión-de-despliegue) y la [arquitectura técnica](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md). No cambia el contrato del bundle ni su perfil.

## Capas y cachés

El `Dockerfile` usa el mismo toolchain Rust 1.99.1 en todas las etapas de compilación:

1. `chef` instala `cargo-chef` 0.1.78 con `--locked`.
2. `planner` genera una receta con los manifiestos, el lockfile, los targets y el perfil de compilación. Copia también `build.rs`, `src/` y `migrations/`.
3. `dependencies` compila la receta con `cargo chef cook --release --locked`. Un cambio solo en el contenido del código deja la receta intacta y permite reutilizar esta capa.
4. `build` incorpora las fuentes reales y las migraciones SQL embebidas, fija `OKF_BUILD_COMMIT` y compila el ejecutable. Cambiar el SHA no invalida las dependencias.
5. `assets` instala las fuentes y certificados fijados por versión en Debian. `runtime` copia únicamente esos archivos, sus avisos de licencia, el directorio de datos y el ejecutable sobre Distroless.

Los mounts de BuildKit conservan el registro de Cargo y los repositorios Git entre construcciones del mismo builder, con acceso exclusivo (`sharing=locked`). **`/src/target` permanece en la capa de dependencias**, de modo que su contenido compilado se exporta a la caché de CI. No depende de un mount que GitHub Actions no conserva automáticamente.

La CI usa `docker/setup-buildx-action@v4` y `docker/build-push-action@v7`, con `cache-from: type=gha,scope=okf-image-linux-amd64` y `cache-to: type=gha,mode=max,scope=okf-image-linux-amd64`. `mode=max` incluye las etapas intermedias. `load: true` carga la imagen para ejecutar `tools/smoke.sh`; no se publica en un registro. Las políticas de acceso, cuota y expulsión de GitHub Actions pueden provocar una construcción sin caché. Los mounts de descarga son locales al builder y no se exportan a GHA.

Cambiar `Cargo.toml`, `Cargo.lock`, el perfil, el toolchain o la estructura de targets puede exigir recompilar dependencias. La primera construcción también debe instalar `cargo-chef`; el beneficio principal aparece en las reconstrucciones. No se usa `--no-cache` en el flujo habitual. `.dockerignore` conserva la lista permitida de fuentes: no envía `.env`, datos privados, `.git` ni `target/` al builder.

El job `test` mantiene una caché Rust independiente de Docker. Se configura con `Swatinem/rust-cache@v2`, prefijo `v1-rust-ci-no-debug` y `cache-bin: false`. Solo en ese job, `CARGO_PROFILE_DEV_DEBUG=0` y `CARGO_PROFILE_TEST_DEBUG=0` evitan guardar símbolos de depuración; conservan las aserciones, los controles de desbordamiento y las demás opciones de las pruebas. El prefijo nuevo impide recuperar la caché anterior con símbolos completos, que agotó el disco del runner al extraer 1 985 837 586 bytes comprimidos en la [ejecución de `b872f75`](https://github.com/jpadillaa/rs_okf_microservice/actions/runs/41827020407). El desarrollo local conserva sus símbolos.

## Imagen de ejecución

La base es `gcr.io/distroless/cc-debian13:nonroot`, fijada por digest en el Dockerfile y el stack. La variante `cc` incluye las bibliotecas dinámicas que requiere el binario Rust; no se usa `static` porque el ejecutable enlaza con glibc y libgcc. La imagen no incluye Cargo, compilador, shell ni gestor de paquetes. `ENTRYPOINT` ejecuta una ruta absoluta en formato JSON.

Se conservan las fuentes DejaVu 2.37-8 y el paquete de certificados 20250419. El proceso usa UID/GID 10001:10001; el UID coincide con el de los volúmenes anteriores. `/data` se prepara con ese propietario para volúmenes nuevos. `WORKDIR /` mantiene un directorio accesible para buscar el `.env` opcional: el directorio predeterminado `/home/nonroot` de Distroless pertenece al UID 65532. La conexión PostgreSQL, la activación de Swagger y los límites siguen siendo configuración de ejecución desde `.env`.

`[profile.release] strip = "symbols"` elimina los símbolos del ejecutable distribuido; las pruebas y el perfil de desarrollo conservan su configuración. Los diagnósticos explícitos del servicio y del gancho de pánicos siguen en el código. No se activa LTO adicional ni se cambia la estrategia de pánico: requerirían mediciones separadas de compilación y comportamiento.

## Uso local

```bash
# API y worker comparten la etiqueta; se construye una sola vez.
docker compose build api
docker compose up -d --no-build
tools/smoke.sh okf-microservice:local
```

En un checkout limpio de una revisión confirmada se puede añadir `--build-arg OKF_BUILD_COMMIT="$(git rev-parse HEAD)"` a la construcción. El valor por defecto es `unknown`, adecuado para fuentes locales sin una revisión confirmada. Para aplicar solo un cambio de `.env`, recrear el servicio con `docker compose up -d --no-build --force-recreate api`; no hace falta recompilar.

La limpieza de las imágenes de esta aplicación no requiere borrar volúmenes ni ejecutar un `docker system prune`. PostgreSQL y los documentos conservan sus volúmenes. La caché del builder se administra por separado y puede contener capas compartidas con otros proyectos.

## Mediciones y verificación

Medición local del 4 de noviembre, Docker Desktop sobre Apple M2, destino Linux arm64. Tamaño de imagen según `docker image inspect .Size` (la misma métrica antes y después); tamaño del ejecutable copiado desde el contenedor. No son tamaños de descarga desde un registro.

| Medida | Imagen anterior | Imagen optimizada |
| --- | ---: | ---: |
| Imagen local | 207 715 696 bytes (207,7 MB) | 104 168 583 bytes (104,2 MB), −49,9 % |
| Ejecutable release | 41 416 640 bytes (41,4 MB) | 31 532 808 bytes (31,5 MB), −23,9 % |
| Reconstrucción sin cambios | No medida | 1,00 s |
| Cambio solo en `src/lib.rs` | No medido | 13,81 s; dependencias `CACHED`, solo se compila el paquete de la aplicación |
| Cambio solo de `OKF_BUILD_COMMIT` | No medido | 13,89 s; dependencias `CACHED` |

La primera construcción con las nuevas capas tardó **622,22 s de pared** en Docker. Cargo informó **2 min 42 s** para las dependencias y **14,49 s** para la aplicación; esos tiempos no sustituyen el tiempo total de Docker. La construcción inicial debe preparar las herramientas y dependencias y no se presenta como una aceleración de un build sin caché. Son observaciones individuales, no promedios ni una garantía para otros equipos o para CI. La optimización demostrada corresponde a las reconstrucciones y al tamaño distribuido.

El cambio de código se midió sobre una copia temporal del contexto permitido, añadiendo un comentario sin efecto; no se alteraron las fuentes del repositorio. Las imágenes temporales se retiraron. La imagen local se construyó desde las fuentes reales con `OKF_BUILD_COMMIT=unknown`, antes de confirmar estos cambios de empaquetado en Git.

Verificaciones completadas:

- `cargo fmt`, `cargo clippy --all-targets --locked -- -D warnings`, las 73 pruebas de macOS y `cargo deny check`; los 26 golden siguen intactos.
- `tools/smoke.sh okf-microservice:local`: PostgreSQL aislado, recepción por API, conversión en el proceso hijo de los cuatro formatos, descarga y validación independiente de los cuatro ZIP, salud sin worker y métricas por formato.
- Compose con los volúmenes existentes: API y worker arrancan, `/health` responde `200` con base, almacenamiento y worker disponibles; Swagger, su JavaScript y OpenAPI responden `200`. Se conserva el trabajo anterior.
- Inspección de UID/GID 10001:10001, directorio `/`, fuente TrueType y certificados CA en el runtime.
- YAML de CI válido; la exportación y restauración de caché GHA entre runners se comprobaron después de publicar, como se detalla a continuación.

## OCR local (6 de noviembre)

El [OCR local](OCR_LOCAL.md) añade a la imagen la etapa `ocr-models`. Descarga los modelos con `ADD --checksum` (revisión y SHA-256 fijados), copia la licencia Apache-2.0 desde la etapa de Debian y fija los permisos (755 el directorio, 644 los archivos) antes de copiarlos al runtime. No se usa `ADD --chmod` en el runtime porque aplicaría el modo también a los directorios padre que crea: `/usr/local` quedó en 644 y el worker no pudo lanzar el proceso hijo («Permission denied»).

Por capas de `docker history` (misma máquina, Linux arm64), frente a la imagen del 4 de noviembre:

- el ejecutable pasa de 31,6 a 44,3 MB, por tract y el código nuevo;
- la capa de modelos añade 10,4 MB;
- la imagen local, según `docker image inspect .Size`, pasa de 104,2 a 141,1 MB.

Al añadir tract cambia la receta de `cargo-chef`, así que la primera construcción vuelve a compilar las dependencias. `tools/smoke.sh` termina con una fase de OCR que convierte `escaneado_ocr.pdf` con `OKF_OCR_ENABLED=true` dentro del contenedor y la valida.

## Verificación de CI y caché remota

La [ejecución de `c4f76d9`](https://github.com/jpadillaa/rs_okf_microservice/actions/runs/41833194408) construyó la imagen sin una caché Docker previa, convirtió y validó los cuatro formatos y exportó 42 objetos de caché, 922 359 565 bytes en total. Su job `test` falló antes de ejecutar pruebas al restaurar la caché Rust antigua y agotar el disco; el fallo motivó la configuración sin símbolos y el prefijo nuevo descritos arriba.

La [ejecución de `76466ee`](https://github.com/jpadillaa/rs_okf_microservice/actions/runs/41836323796), en runners nuevos, pasó los tres jobs: `test`, `dependencies` e `image`. El log de Docker muestra la importación del manifiesto GHA y `CACHED` en `cargo chef cook`; solo compila `okf-microservice`, porque cambió `OKF_BUILD_COMMIT`. La imagen resultante vuelve a superar las cuatro conversiones y validaciones.

| Medida en GitHub Actions, Linux amd64 | Primera construcción (`c4f76d9`) | Con caché (`76466ee`) |
| --- | ---: | ---: |
| Paso «Construir la imagen» | 19 min 00 s | 1 min 46 s (−90,7 %) |
| Job `image` completo | 19 min 50 s | 2 min 26 s |
| Prueba de humo de cuatro formatos | 16 s | 16 s |
| Dependencias de release | 13 min 50 s de Cargo | Capa `CACHED` |

Estos tiempos incluyen las operaciones indicadas por cada fila y no son comparables directamente con el equipo arm64 local. En la primera ejecución, exportar la caché tomó 138,2 s. La segunda confirma reutilización entre commits y runners; no depende de que un runner conserve su disco.

Con la corrección, `test` pasó las **75 pruebas Linux**, formato y Clippy. Al crear la nueva caché por primera vez, el job completo tardó 21 min 04 s; guardó **606 576 970 bytes**, frente a los 1 985 837 586 de la caché anterior. La ejecución local previa al commit también pasó formato, Clippy, las 73 pruebas macOS y la auditoría. No se cambiaron los golden ni el perfil documental.

## Fuentes

- [Docker: optimización de caché](https://docs.docker.com/build/cache/optimize/).
- [Docker: caché en GitHub Actions y límites de los mounts](https://docs.docker.com/build/ci/github-actions/cache/).
- [cargo-chef: receta y reutilización de dependencias](https://github.com/LukeMathWalker/cargo-chef).
- [Cargo: perfiles y strip](https://doc.rust-lang.org/cargo/reference/profiles.html).
- [Distroless: imágenes y ejecución sin shell](https://github.com/GoogleContainerTools/distroless).

# Imagen única de la API y el worker. Bases por digest, herramientas y paquetes
# por versión, dependencias por Cargo.lock. Ver docs/decisiones/COMPILACION_DOCKER.md.
ARG RUST_IMAGE=rust:1.99.1-slim-trixie@sha256:b8e062b1c3a80183df83d9ba187cadd2b5ab84bc78d453dbfeb2fccd5f982247
ARG ASSETS_IMAGE=debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian13:nonroot@sha256:08476c244888bfb425bae1ae4dd7c17f093e972aab8426883d97f3c6f280f7c1

FROM ${RUST_IMAGE} AS chef
RUN --mount=type=cache,id=okf-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=okf-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    cargo install cargo-chef --version 0.1.78 --locked
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY migrations ./migrations
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS dependencies
COPY --from=planner /src/recipe.json recipe.json
# target permanece en esta capa: la caché de CI exporta las dependencias compiladas.
# Los mounts conservan las descargas en el builder local, pero no se exportan a GHA.
RUN --mount=type=cache,id=okf-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=okf-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    cargo chef cook --release --locked --recipe-path recipe.json

FROM dependencies AS build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY migrations ./migrations
# La revisión afecta solo al ejecutable, nunca a la capa de dependencias.
ARG OKF_BUILD_COMMIT=unknown
ENV OKF_BUILD_COMMIT=$OKF_BUILD_COMMIT
RUN --mount=type=cache,id=okf-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=okf-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked \
    && cp target/release/okf-microservice /okf-microservice

FROM ${ASSETS_IMAGE} AS assets
# Conserva las mismas fuentes y certificados del runtime anterior.
RUN apt-get update \
    && apt-get install -y --no-install-recommends fonts-dejavu-core=2.37-8 ca-certificates=20250419 \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 /data

FROM ${ASSETS_IMAGE} AS ocr-models
# Modelos del OCR local (PaddleOCR en ONNX, Apache-2.0), fijados por revisión y SHA-256
# como en tools/ocr_models.sh; se usan solo con OKF_OCR_ENABLED=true. La ficha del
# repositorio declara el origen y la licencia, cuyo texto acompaña a los modelos.
# Los permisos se fijan aquí: `ADD --chmod` los aplicaría también a los directorios
# padre que crea, y el UID 10001 no podría recorrerlos.
ARG OCR_MODELS=https://huggingface.co/monkt/paddleocr-onnx/resolve/7b02d0a30a07ba2b92ad1ff5a8941ae2c633de65
ADD --checksum=sha256:ee40e80071ba3a320d4efda75f3e22047a7d049e9bf7bcaaf9daea23fc21b935 \
    ${OCR_MODELS}/detection/v3/det.onnx /ocr/det.onnx
ADD --checksum=sha256:614ffc2d6d3902d360fad7f1b0dd455ee45e877069d14c4e51a99dc4ef144409 \
    ${OCR_MODELS}/languages/latin/rec.onnx /ocr/rec.onnx
ADD --checksum=sha256:3c0a8a79b612653c25f765271714f71281e4e955962c153e272b7b8c1d2b13ff \
    ${OCR_MODELS}/languages/latin/dict.txt /ocr/dict.txt
ADD --checksum=sha256:783815b2d4127bbdf2847fac50d4dfa3b4ec58508324e43a7b41de66994f6611 \
    ${OCR_MODELS}/README.md /ocr/README.md
RUN cp /usr/share/common-licenses/Apache-2.0 /ocr/LICENSE \
    && chmod 755 /ocr && chmod 644 /ocr/*

FROM ${RUNTIME_IMAGE} AS runtime
# Distroless nonroot parte de /home/nonroot (UID 65532); nuestro UID es 10001.
# Un directorio accesible permite buscar el .env opcional como en el runtime anterior.
WORKDIR /
COPY --from=assets /usr/share/fonts/truetype/dejavu /usr/share/fonts/truetype/dejavu
COPY --from=assets /usr/share/doc/fonts-dejavu-core /usr/share/doc/fonts-dejavu-core
COPY --from=assets /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=assets --chown=10001:10001 /data /data
COPY --from=ocr-models /ocr /usr/local/share/okf/ocr
COPY --from=build /okf-microservice /usr/local/bin/okf-microservice
ENV OKF_DATA_DIR=/data \
    OKF_FONT_PATH=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
    OKF_OCR_MODELS_DIR=/usr/local/share/okf/ocr
USER 10001:10001
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/okf-microservice"]
CMD ["api", "--bind", "0.0.0.0:8080"]

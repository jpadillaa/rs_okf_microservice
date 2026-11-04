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

FROM ${RUNTIME_IMAGE} AS runtime
# Distroless nonroot parte de /home/nonroot (UID 65532); nuestro UID es 10001.
# Un directorio accesible permite buscar el .env opcional como en el runtime anterior.
WORKDIR /
COPY --from=assets /usr/share/fonts/truetype/dejavu /usr/share/fonts/truetype/dejavu
COPY --from=assets /usr/share/doc/fonts-dejavu-core /usr/share/doc/fonts-dejavu-core
COPY --from=assets /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=assets --chown=10001:10001 /data /data
COPY --from=build /okf-microservice /usr/local/bin/okf-microservice
ENV OKF_DATA_DIR=/data \
    OKF_FONT_PATH=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
USER 10001:10001
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/okf-microservice"]
CMD ["api", "--bind", "0.0.0.0:8080"]

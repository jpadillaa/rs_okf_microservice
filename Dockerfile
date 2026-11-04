# Imagen única de la API y el worker (plan 1, R3): el mismo ejecutable con los
# subcomandos `api` y `worker`. Todas las versiones están fijadas: las imágenes base por
# digest, el toolchain por `rust-version` de Cargo.toml, las dependencias por Cargo.lock
# y la fuente del renderizado PPTX por su versión de paquete.
ARG RUST_IMAGE=rust:1.99.1-slim-trixie@sha256:b8e062b1c3a80183df83d9ba187cadd2b5ab84bc78d453dbfeb2fccd5f982247
ARG RUNTIME_IMAGE=debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

FROM ${RUST_IMAGE} AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
ARG OKF_BUILD_COMMIT=unknown
ENV OKF_BUILD_COMMIT=$OKF_BUILD_COMMIT
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked \
    && cp target/release/okf-microservice /okf-microservice

FROM ${RUNTIME_IMAGE}
# Fuente TrueType de las vistas PPTX (OKF_FONT_PATH); la misma versión que usa la CI.
RUN apt-get update \
    && apt-get install -y --no-install-recommends fonts-dejavu-core=2.37-8 ca-certificates=20250419 \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --home-dir /data okf \
    && mkdir /data \
    && chown okf /data
COPY --from=build /okf-microservice /usr/local/bin/okf-microservice
ENV OKF_DATA_DIR=/data \
    OKF_FONT_PATH=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
USER okf
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["okf-microservice"]
CMD ["api", "--bind", "0.0.0.0:8080"]

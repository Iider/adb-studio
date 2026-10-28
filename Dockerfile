# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run check && npm run build

FROM rust:1.96-bookworm AS backend
ARG TARGETARCH
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY workflows/ workflows/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=adb-studio-target-${TARGETARCH} \
    cargo build --locked --release && mkdir -p /out && cp target/release/adb-studio /out/adb-studio

FROM backend AS checks
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
COPY scripts/ scripts/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=adb-studio-target-${TARGETARCH} \
    cargo test --locked && cargo build --locked \
    && python3 scripts/check-integration.py \
    && python3 scripts/check-platform.py \
    && python3 scripts/check-vision.py \
    && python3 scripts/check-mirror.py \
    && python3 scripts/check-workflow-export.py

FROM python:3.12-slim-bookworm AS mirror
WORKDIR /src
COPY scripts/setup-mirror.py scripts/setup-mirror.py
RUN python3 scripts/setup-mirror.py

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends adb ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /out/adb-studio ./adb-studio
COPY --from=web /src/web/build ./web/build
COPY --from=mirror /src/data/bin/scrcpy-server-v3.1 ./bin/scrcpy-server-v3.1
ENV ADB_STUDIO_ROOT=/app ADB_STUDIO_DATA=/data ADB_STUDIO_ADB=/usr/bin/adb \
    ADB_STUDIO_HOST=0.0.0.0 HOME=/data ADB_SERVER_SOCKET=tcp:5038
VOLUME ["/data"]
EXPOSE 17841
STOPSIGNAL SIGTERM
CMD ["/app/adb-studio"]

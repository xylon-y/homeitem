# ---------- 阶段 1: 前端构建 (Trunk + wasm) ----------
FROM rust:1.85-slim AS frontend-builder
RUN rustup target add wasm32-unknown-unknown \
 && cargo install trunk --locked --version 0.22
WORKDIR /build
COPY crates/common /build/crates/common
COPY crates/frontend /build/crates/frontend
COPY Cargo.toml Cargo.lock /build/
RUN cd crates/frontend && trunk build --release

# ---------- 阶段 2: 后端构建 (rust-embed 打包前端产物) ----------
FROM rust:1.85-slim AS backend-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock /build/
COPY crates/common /build/crates/common
COPY crates/backend /build/crates/backend
# 预置前端产物（rust-embed 编译期读取）
COPY --from=frontend-builder /build/crates/frontend/dist /build/crates/frontend/dist
RUN cargo build --release -p backend

# ---------- 阶段 3: 运行镜像 (debian-slim, 非 root) ----------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
# 非 root 运行
RUN useradd --system --create-home --home-dir /app --uid 10001 homeitem
WORKDIR /app
COPY --from=backend-builder /build/target/release/backend /app/backend
# 数据卷：SQLite 数据库 + 附件上传目录
RUN mkdir -p /data/uploads && chown -R homeitem:homeitem /app /data
VOLUME ["/data"]
ENV HOMEITEM_DB_PATH=/data/homeitem.db \
    HOMEITEM_UPLOAD_DIR=/data/uploads \
    HOMEITEM_LISTEN_ADDR=0.0.0.0:7745
USER homeitem
EXPOSE 7745
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD curl -fsS http://127.0.0.1:7745/api/v1/health || exit 1
ENTRYPOINT ["/app/backend"]

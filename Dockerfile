# 构建阶段
FROM rust:1.85-bookworm AS builder

# 安装构建依赖
RUN apt-get update && \
    apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# 构建release版本
RUN cargo build --release

# 运行阶段
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin
COPY --from=builder /app/target/release/httpdispatcher .

RUN useradd -ms /bin/bash appuser && \
    chown -R appuser:appuser /usr/local/bin

USER appuser

EXPOSE 9090

CMD ["/usr/local/bin/httpdispatcher", "--config", "/usr/local/bin/config.yaml"]
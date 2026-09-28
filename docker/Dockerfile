FROM rust:1.98-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src/ src/

RUN cargo build --release && \
    strip target/release/conduitd

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --no-create-home --shell /bin/false conduit

COPY --from=builder /app/target/release/conduitd /usr/local/bin/conduitd

USER conduit

EXPOSE 1883 8883 9090

ENV RUST_LOG=conduitd=info
ENV CONDUIT_PLAIN_ENABLED=1

CMD ["conduitd"]
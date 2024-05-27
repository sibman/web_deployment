FROM rust:alpine3.20 AS builder

WORKDIR /app

COPY Cargo.lock ./
COPY Cargo.toml ./

COPY rest_actuator/Cargo.toml ./rest_actuator/
COPY rest_actuator/src ./rest_actuator/src/

COPY rest_service_lib/Cargo.toml ./rest_service_lib/
COPY rest_service_lib/Secrets.toml ./rest_service_lib/
COPY rest_service_lib/src ./rest_service_lib/src/

COPY rest_service/Cargo.toml ./rest_service/
COPY rest_service/src ./rest_service/src/

#RUN apt-get update && apt-get install -y pkg-config libssl-dev curl
# Install necessary packages
RUN apk add --no-cache \
    pkgconfig \
    openssl \
    openssl-dev \
    curl \
    build-base \
    musl-dev

# Set up Rust environment
RUN rustup default stable \
    && rustup target add x86_64-unknown-linux-musl

# Set PKG_CONFIG_PATH to ensure openssl can be found
ENV PKG_CONFIG_PATH=/usr/lib/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig

RUN cargo build --release -p rest_service

# Create a user and set permissions
RUN adduser -D -h /home/restuser -s /bin/sh restuser \
    && mkdir -p /home/restuser \
    && chown -R restuser:restuser /home/restuser \
    && chown restuser:restuser /app/target/release/rest_service

FROM scratch

# Copy the necessary files from the builder stage
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group
COPY --from=builder /home/restuser /home/restuser
COPY --from=builder /app/target/release/rest_service /home/restuser/rest_service

USER restuser
WORKDIR /home/restuser

# Verify the presence and permissions of the binary
#RUN ls -l /home/restuser/rest_service

EXPOSE 3000

ENTRYPOINT ["/home/restuser/rest_service"]

# Set up base image matching the runtime env (to avoid glibc compat issues)
FROM --platform=$TARGETOS/$TARGETARCH debian:stable-slim
SHELL ["/bin/bash", "-c"]

# Tell debian we are not in an interactive environment
ENV DEBIAN_FRONTEND=noninteractive
WORKDIR /build

# Install build dependencies
RUN apt update \
    && apt install -y tar curl gcc g++ pkg-config libssl-dev

# Install rustup (passing '-y' flag to accept defaults)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && source $HOME/.cargo/env \
    && cargo --version

# Copy files from the working directory (repo) to inside the container
COPY . .

CMD ["bash", "-c", "source $HOME/.cargo/env && cargo build --release"]

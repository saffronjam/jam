FROM rust:1.97.1-bookworm
RUN apt-get update -qq && apt-get install -y -qq python3 clang pkg-config gcc-x86-64-linux-gnu gcc-aarch64-linux-gnu && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

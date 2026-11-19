# Cross-compiles the Windows build (windfalld.exe, windfallctl.exe) and builds the installer, all in a container.
FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends mingw-w64 nsis && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-pc-windows-gnu
ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
WORKDIR /src

# context-capsule — capture-rs: a Linux box to build and test the Linux recorder (also used by CI).
FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config clang libclang-dev cmake \
      libdbus-1-dev libpipewire-0.3-dev libspa-0.2-dev \
      libasound2-dev libx11-dev libxcb1-dev libxkbcommon-dev \
      tesseract-ocr tesseract-ocr-eng \
      xvfb at-spi2-core dbus-x11 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy rustfmt
# whisper.cpp: a portable build (CPU detection inside VMs picks flags the compiler then rejects).
ENV GGML_NATIVE=OFF
WORKDIR /src

FROM rust:1.97.1-bookworm
RUN apt-get update -qq && apt-get install -y -qq python3 clang pkg-config libvulkan1 mesa-vulkan-drivers libxkbcommon-dev libxkbcommon-x11-0 libx11-dev libxi-dev libxcursor-dev libxrandr-dev xvfb fonts-noto-color-emoji fonts-noto-cjk && rm -rf /var/lib/apt/lists/*

# Waddle for the browser: the dashboard and your models, served from a container.
#   docker run -d -p 5050:5050 -p 5001:5001 -p 5002:5002 -v waddle:/data ghcr.io/soleritu/waddle
# Then open the link the container prints: docker logs <container>
#
# Two flavours, picked with --build-arg FLAVOR:
#   cpu    (default) small. Runs models on the processor, or on NVIDIA GPUs with --gpus all:
#          Waddle downloads llama.cpp's CUDA build and its runtime into /data when it sees one.
#   vulkan adds Mesa's Vulkan drivers for AMD and Intel GPUs. Pass the GPU with --device /dev/dri.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
RUN cargo build --release --no-default-features

FROM ubuntu:26.04
ARG FLAVOR=cpu
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libgomp1 libcurl4t64 \
 && if [ "$FLAVOR" = "vulkan" ]; then apt-get install -y --no-install-recommends libvulkan1 mesa-vulkan-drivers; fi \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/waddle /usr/local/bin/waddle
ENV WADDLE_HOME=/data WADDLE_WEB=1
VOLUME /data
EXPOSE 5050 5001 5002
CMD ["waddle"]

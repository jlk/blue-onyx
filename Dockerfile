# Use NVIDIA CUDA runtime base so libcublasLt and other CUDA libs are available
# when running with --gpus. Use debian:bookworm-slim for CPU-only builds.
#FROM nvidia/cuda:12.6.0-runtime-ubuntu22.04
FROM nvidia/cuda:13.1.1-cudnn-runtime-ubuntu24.04

LABEL maintainer="xnorpx@outlook.com"
LABEL description="Blue Onyx docker container"

ENV TARGET_FOLDER=/models

# Install dependencies (openssl/ca-certificates for HTTPS, etc.)
RUN apt-get update && \
    apt-get install -y --no-install-recommends openssl ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --no-log-init blueonyx

# Set working directory
WORKDIR /app

RUN chown blueonyx:blueonyx /app

# Copy application files and set ownership
COPY --chown=blueonyx:blueonyx target/release/blue_onyx target/release/libonnxruntime.so target/release/libonnxruntime_providers_cuda.so target/release/libonnxruntime_providers_shared.so  ./

# Copy model files and set ownership
COPY --chown=blueonyx:blueonyx models/* ./

# Switch to non-root user
USER blueonyx

# Expose port
EXPOSE 32168

# Define entrypoint
ENTRYPOINT ["./blue_onyx"]

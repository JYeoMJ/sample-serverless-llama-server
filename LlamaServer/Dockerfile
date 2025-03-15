FROM public.ecr.aws/sam/build-python3.12:latest

# Install system dependencies
RUN dnf install -y \
    gcc-c++ \
    make \
    cmake \
    git

WORKDIR /build

# Clone llama.cpp and initialize submodules
RUN git clone https://github.com/ggerganov/llama.cpp.git && \
    cd llama.cpp && \
    git checkout b4689

# Build the wheel
WORKDIR /build/llama.cpp
ENV CMAKE_ARGS="-DBUILD_SHARED_LIBS=OFF -DGGML_NATIVE=OFF -DGGML_LTO=ON -DGGML_AVX2=ON -DGGML_AVX512=OFF"

RUN cmake -B build ${CMAKE_ARGS} && \
    cmake --build build --config Release -t llama-server

# Prepare layer structure
RUN mkdir -p /opt/{bin,lib} && \
    cp build/bin/llama-server /opt/bin/ && \
    cp /lib64/libgomp.so.1 /opt/lib
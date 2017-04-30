FROM debian

ENV DEBIAN_FRONTEND="noninteractive" RUST_NIGHTLY_NAME="rust-nightly-x86_64-unknown-linux-gnu"

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
			binutils-dev \
      build-essential \
      ca-certificates \
			cmake \
      curl \
      git \
			libcurl4-openssl-dev \
			libdw-dev \
			libelf-dev \
      libiberty-dev \
			pkg-config \
			python \
			unzip \
			zlib1g-dev && \
    curl -sOSL https://static.rust-lang.org/dist/${RUST_NIGHTLY_NAME}.tar.gz && \
    curl -s https://static.rust-lang.org/dist/${RUST_NIGHTLY_NAME}.tar.gz.sha256 | sha256sum -c - && \
    tar -xzf ${RUST_NIGHTLY_NAME}.tar.gz && \
    ./${RUST_NIGHTLY_NAME}/install.sh && \
		curl -sOSL https://github.com/SimonKagstrom/kcov/archive/master.zip && \
    unzip master.zip && \
    cd kcov-master && \
    mkdir build && \
    cd build && \
    cmake .. && \
    make && \
    make install && \
    cd ../.. && \
    rm -rf kcov-master && \
		rm master.zip && \
    rm -rf \
      ${RUST_NIGHTLY_NAME} \
      ${RUST_NIGHTLY_NAME}.tar.gz \
      /tmp/* \
      /var/tmp/* \
      /var/lib/apt/lists/* && \
    apt-get remove --purge -y curl \
      binutils-dev \
      build-essential \
			cmake \
      curl \
			libcurl4-openssl-dev \
			libdw-dev \
			libelf-dev \
      libiberty-dev \
			python \
			unzip \
			zlib1g-dev && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
			libssl-dev \
			libsqlite3-dev \

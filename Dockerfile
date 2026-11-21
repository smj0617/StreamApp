# syntax=docker/dockerfile:1
#
# StreamApp 서버 컨테이너 이미지(과제 ⑤ 로컬 오토스케일). 리눅스 서버 번들(.github/workflows/release-server.yml)과
# 같은 방법으로 만든다: dx bundle --web --release --package web → server(실행 파일) + public/(화면 파일).
#
#   docker build -t streamapp:local .
#   docker run --rm -p 8080:8080 -v streamapp-data:/data streamapp:local
#
# - 빌드 단계: rust-toolchain.toml의 Rust(1.99.1)와 같은 공식 이미지(trixie), wasm32 타깃, dx 0.7.10(체크섬 확인).
#   dx 0.7.10의 미리 빌드된 리눅스 파일은 glibc 2.39 이상을 요구해 bookworm(2.36)에서는 실행되지 않는다.
# - 실행 단계: debian trixie-slim(빌드와 같은 glibc 2.41) + ca-certificates(토스페이먼츠 HTTPS), 비루트 사용자.
# - 상태(SQLite DB, 녹화방송 영상)는 /data에 둔다. 쿠버네티스에서는 공유 볼륨(PVC)을 붙인다(k8s/app/streamapp.yaml).
# - HEALTHCHECK는 두지 않는다. 쿠버네티스의 readiness·liveness probe(GET /healthz)가 맡는다.
# - BuildKit 캐시 마운트로 cargo 내려받기와 target/을 빌드 사이에 재사용한다(Docker Desktop은 BuildKit이 기본).

ARG RUST_VERSION=1.99.1

########## 빌드 ##########
FROM rust:${RUST_VERSION}-trixie AS build

# dx(dioxus-cli)는 로컬 개발·CI와 같은 0.7.10으로 고정하고, GitHub 릴리스의 미리 빌드된 파일을 체크섬으로 확인한다.
ARG DX_VERSION=0.7.10
ARG DX_SHA256_AMD64=4363e4ed2a3f1eb7f4d38d2d59aed59ce43271c44c16b425e92c89a64761fbe7
ARG DX_SHA256_ARM64=8f1a17d3218700ffbe15e6540d936a178b2556fc801121a31082e3ba4ab9ef55
RUN set -eux; \
    case "$(dpkg --print-architecture)" in \
      amd64) target=x86_64-unknown-linux-gnu; sha="${DX_SHA256_AMD64}" ;; \
      arm64) target=aarch64-unknown-linux-gnu; sha="${DX_SHA256_ARM64}" ;; \
      *) echo "지원하지 않는 아키텍처" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/dx.tar.gz "https://github.com/DioxusLabs/dioxus/releases/download/v${DX_VERSION}/dx-${target}.tar.gz"; \
    echo "${sha}  /tmp/dx.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/dx.tar.gz -C /usr/local/cargo/bin dx; \
    rm /tmp/dx.tar.gz; \
    dx --version

WORKDIR /src
# rust-toolchain.toml에 적힌 버전·타깃(wasm32-unknown-unknown)·구성요소를 설치한다(CI와 같은 방법).
COPY rust-toolchain.toml ./
RUN rustup toolchain install && rustc --version

COPY . .
# 풀스택 번들: 서버(web 패키지, server 기능)와 화면(wasm, web 기능). /out/server와 /out/public/이 생긴다.
# dx가 처음 빌드할 때 wasm-bindgen 등 보조 도구를 내려받는다(인터넷 필요).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/root/.cache \
    dx bundle --web --release --package web --out-dir /out \
    && test -x /out/server && test -d /out/public

########## 실행 ##########
FROM debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 비루트 사용자. 쿠버네티스 매니페스트의 runAsUser·fsGroup(10001)과 같다.
RUN groupadd --system --gid 10001 streamapp \
    && useradd --system --uid 10001 --gid streamapp --home-dir /app --shell /usr/sbin/nologin streamapp \
    && mkdir -p /app /data \
    && chown streamapp:streamapp /data

WORKDIR /app
# 서버는 실행 파일 옆의 public/을 화면 파일로 내준다.
COPY --from=build /out/server /app/server
COPY --from=build /out/public /app/public

# 듣는 주소(dioxus::serve가 IP·PORT를 읽는다)와 상태 저장 위치. 나머지 환경변수는 README ⑤ 절과
# k8s/app/streamapp.yaml(ConfigMap)을 본다.
ENV IP=0.0.0.0 \
    PORT=8080 \
    STREAMAPP_DB=/data/streamapp.db \
    STREAMAPP_RECORDINGS_DIR=/data/recordings

USER 10001:10001
EXPOSE 8080
CMD ["/app/server"]

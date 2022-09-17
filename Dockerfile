#------------------------------------------------------------------------------
FROM rust:slim-bullseye AS base

RUN mkdir /cargo
COPY Cargo.toml Cargo.lock /cargo/
COPY src /src

#------------------------------------------------------------------------------
FROM rust:slim-bullseye AS build-bullseye

RUN apt-get update && apt-get install -y libssl-dev pkg-config

RUN groupadd -g 1000 rust \
    && useradd -u 1000 -g 1000 rust \
    && mkdir /build \
    && chown -R 1000:1000 /build

USER 1000
WORKDIR /build

COPY --from=base --chown=1000:1000 /cargo /build
RUN cd /build && mkdir src && echo "fn main(){}" > src/main.rs && cargo build --target-dir /target/bullseye --release

COPY --from=base --chown=1000:1000 /src /build/src
RUN cd /build && cargo build --target-dir /target/bullseye --release && cp /target/bullseye/release/loadwork /build/loadwork-release

#------------------------------------------------------------------------------
#FROM rust:alpine3.14 AS build-alpine
FROM ekidd/rust-musl-builder:stable AS build-alpine

#USER root
#RUN sudo apt-get update && sudo apt-get install -y ...

USER 1000
COPY --from=base --chown=1000:1000 /cargo /build
RUN cd /build && mkdir src && echo "fn main(){}" > src/main.rs && cargo build --target-dir /target/alpine --release

COPY --from=base --chown=1000:1000 /src /build/src
RUN cd /build && cargo build --target-dir /target/alpine --release && cp /target/alpine/x86_64-unknown-linux-musl/release/loadwork /build/loadwork-release

#------------------------------------------------------------------------------
FROM debian:bullseye-slim AS main

RUN apt update -y \
    && apt install -y curl unzip \
    && apt clean \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir /build.tmp

RUN cd /build.tmp \
    && curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" \
    && unzip awscliv2.zip \
    && ./aws/install

RUN cd /build.tmp \
    && curl https://dl.min.io/client/mc/release/linux-amd64/mc -o /usr/local/bin/mc \
    && chmod a+x /usr/local/bin/mc

RUN cd /build.tmp \
    && curl https://downloads.mongodb.com/compass/mongodb-mongosh_1.1.9_amd64.deb -o  mongodb-mongosh_1.1.9_amd64.deb \
    && dpkg -i mongodb-mongosh_1.1.9_amd64.deb

RUN cd / && rm -rf /build.tmp

COPY --from=build-bullseye /build/loadwork-release /loadwork-bullseye-release
COPY --from=build-alpine   /build/loadwork-release /loadwork-alpine-release
RUN chmod 755 /loadwork-*

CMD [ /bin/sh ]

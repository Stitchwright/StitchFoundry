# The StitchFoundry cloud service (apps/stitchfoundry-cloud) as a container, for Railway or any
# container host. Build from the repository root:
#   docker build -f packaging/cloud/Dockerfile -t stitchfoundry-cloud .
# Settings come from the environment (docs/cloud.md); nothing secret is built in.
FROM rust:1.95-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p stitchfoundry-cloud

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/stitchfoundry-cloud /usr/local/bin/stitchfoundry-cloud
# Railway gives the port in PORT; the data folder is a volume mounted at /data.
ENV SF_CLOUD_DATA=/data
EXPOSE 8080
CMD ["stitchfoundry-cloud"]

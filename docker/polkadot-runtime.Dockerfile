FROM debian:12-slim

RUN apt-get update \
	&& apt-get install -y --no-install-recommends \
		ca-certificates \
		libssl3 \
	&& rm -rf /var/lib/apt/lists/*

WORKDIR /opt/bin

ENTRYPOINT ["/opt/bin/polkadot"]

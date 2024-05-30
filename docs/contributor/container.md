# Using Containers

Using containers via **Podman** or **Docker** brings benefit, whether it is to build a container image or run a node
while keeping a minimum footprint on your local system.

This document mentions using `podman` or `docker`. Those are usually interchangeable and it is encouraged using
preferably **Podman**. If you have podman installed and want to use all the commands mentioned below, you can simply
create an alias with `alias docker=podman`.

There are a few options to build a node within a container and inject a binary inside an image.

## Parity built container image

Parity builds and publishes a container image that can be found as `docker.io/parity/polkadot-parachain`.

## Parity CI image

Parity maintains and uses internally a generic "CI" image that can be used as a base to build binaries: [Parity CI
container image](https://github.com/paritytech/scripts/tree/master/dockerfiles/ci-unified):

The command below allows building a Linux binary without having to even install Rust or any dependency locally:

```bash
docker run --rm -it \
    -w /polkadot-sdk \
    -v $(pwd):/polkadot-sdk \
    docker.io/paritytech/ci-unified:bullseye-1.77.0-2024-04-10-v20240408 \
    cargo build --release --locked -p polkadot-parachain-bin --bin polkadot-parachain
sudo chown -R $(id -u):$(id -g) target/
```

## Injected image

Injecting a binary inside a base image is the quickest option to get a working container image. This only works if you
were able to build a Linux binary, either locally, or using a container as described above.

After building a Linux binary (`polkadot-parachain`) with cargo or with Parity CI image as documented above, the
following command allows producing a new container image where the compiled binary is injected:

```bash
ARTIFACTS_FOLDER=./target/release /docker/scripts/build-injected.sh
```

## Container build

Alternatively, you can build an image with a builder pattern. This options takes a while but offers a simple method for
anyone to get a working container image without requiring any of the Rust toolchain installed locally.

```bash
docker build \
	--tag $OWNER/$IMAGE_NAME \
 --file ./docker/dockerfiles/polkadot-parachain/polkadot-parachain_builder.Dockerfile .
```

You may then run your new container:

```bash
docker run --rm -it \
	$OWNER/$IMAGE_NAME \
		--collator --tmp \
		--execution wasm \
		--chain /specs/asset-hub-westend.json
```

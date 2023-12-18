#!/bin/bash
set -x

mkdir -p ./artifacts
mkdir -p ./artifacts/bridges/
cp target/release/polkadot ./artifacts/.
cp target/release/polkadot-prepare-worker ./artifacts/.
cp target/release/polkadot-execute-worker ./artifacts/.
cp target/release/polkadot-parachain ./artifacts/.
cp -r bridges/zombienet ./artifacts/bridges/.
cp -r cumulus/scripts ./artifacts/bridges/cumulus-scripts
cp -r cumulus/zombienet/bridge-hubs ./artifacts/bridges/cumulus-brige-hubs

#docker build -f docker/dockerfiles/polkadot/polkadot_builder.Dockerfile . -t polkadot-builder \
#	--build-arg VCS_REF=x \
#	--build-arg BUILD_DATE=y \
#	--build-arg IMAGE_NAME=z \
#	--build-arg ZOMBIENET_IMAGE=docker.io/paritytech/zombienet:v1.3.79

docker build -f docker/dockerfiles/bridges-zombienet-debug_injected.Dockerfile . -t bzn \
	--build-arg VCS_REF=x \
	--build-arg BUILD_DATE=y \
	--build-arg IMAGE_NAME=z \
	--build-arg ZOMBIENET_IMAGE=docker.io/paritytech/zombienet:v1.3.79

# docker run -it --entrypoint /bin/bash paritytech/substrate-relay:v2023-11-07-rococo-westend-initial-relayer:
#
# user@0fd8c0122d04:~$ ls -la /usr/lib/x86_64-linux-gnu/libstdc++.so.6
# lrwxrwxrwx 1 root root 19 Jul  9 06:45 /usr/lib/x86_64-linux-gnu/libstdc++.so.6 -> libstdc++.so.6.0.28

# docker run -it --entrypoint /bin/bash docker.io/paritytech/zombienet:v1.3.79
#
# nonroot@39db69fc2525:~/zombie-net$ ls -la /usr/lib/x86_64-linux-gnu/libstdc++.so.6
# lrwxrwxrwx 1 root root 19 Jan 10  2021 /usr/lib/x86_64-linux-gnu/libstdc++.so.6 -> libstdc++.so.6.0.28

# docker run -it --entrypoint /bin/bash -p 9942:9942 -p 9910:9910 -p 8943:8943 -p 9945:9945 -p 9010:9010 -p 8945:8945 bzn

# docker run -it --entrypoint /bin/bash -p 9942:9942 -p 9910:9910 -p 8943:8943 -p 9945:9945 -p 9010:9010 -p 8945:8945 docker.io/paritypr/bridges-zombienet-tests:2439-fcc42168
#!/usr/bin/env bash

trap "echo; exit" INT
trap "echo; exit" HUP

DOCKER_DEFAULT_PLATFORM=$DOCKER_DEFAULT_PLATFORM
# fallback to this default platform only if no platform argument specified
DOCKER_FALLBACK_PLATFORM=linux/amd64
echo ${DOCKER_DEFAULT_PLATFORM:=$DOCKER_FALLBACK_PLATFORM}

# if they call this script from project root or from within docker/ folder then
# in both cases the PARENT_DIR will refer to the project root. it supports calls from symlinks
PROJECT_ROOT=$(dirname "$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")")

# verify that we did infact successfully changed to the project root directory
if [ -e ./substrate/docker/substrate_builder.Dockerfile ]
then
  echo "already in project root"
else
  echo "switching to project root"
  cd $PROJECT_ROOT
fi

# Find the current version from Cargo.toml
VERSION=`grep "^version" $PROJECT_ROOT/substrate/bin/node/cli/Cargo.toml | egrep -o "([0-9\.]+)"`
GITUSER=parity
GITREPO=substrate

# https://stackoverflow.com/a/25554904/3208553
# note: the option `--platform linux/x86_64` is required on Apple Silicon architectures otherwise get error
# `qemu-x86_64: Could not open '/lib64/ld-linux-x86-64.so.2': No such file or directory` or
# `rosetta error: failed to open elf at /lib64/ld-linux-x86-64.so.2`
# since the requested images platform (linux/amd64) may not match the detected host platform
# that uses (linux/arm64/v8)
#
# see https://stackoverflow.com/questions/68630526/lib64-ld-linux-x86-64-so-2-no-such-file-or-directory-error

# Build the image
echo "Building ${GITUSER}/${GITREPO}:latest docker image, hang on!"
# improve the docker logs to actually allow debugging with BuildKit enabled since build time may take an hour
export BUILDKIT_PROGRESS=plain
export DOCKER_BUILDKIT=1

try_build() {
  echo "Building for $1 architecture"
  PLATFORM=$1
# heredoc cannot be indented
bash -e << TRY
  time docker build \
    --platform $PLATFORM \
    -f $PROJECT_ROOT/substrate/docker/substrate_builder.Dockerfile \
    -t ${GITUSER}/${GITREPO}:latest ./
TRY
  if [ $? -ne 0 ]; then
    printf "\n*** Detected error running 'docker build'. Trying 'docker buildx' instead...\n"

# heredoc cannot be indented
bash -e << TRY
  time docker buildx build \
    --platform $PLATFORM \
    -f $PROJECT_ROOT/substrate/docker/substrate_builder.Dockerfile \
    -t ${GITUSER}/${GITREPO}:latest ./
TRY
    if [ $? -ne 0 ]; then
      printf "\n*** Detected unknown error running 'docker buildx'.\n"
      exit 1
    fi

  fi
}

# optionally use `docker build --no-cache ...`
# reference: https://github.com/paritytech/scripts/blob/master/get-substrate.sh
if [[ "$OSTYPE" == "darwin"* ]]; then
  set -e

  echo "Mac OS (Darwin) detected."
  if [[ $(uname -m) == 'arm64' ]]; then
    echo "Detected Apple Silicon"
    # emulate using `linux/x86_64` for Apple Silicon to avoid error with `/lib64/ld-linux-x86-64.so.2`
    DOCKER_DEFAULT_PLATFORM=linux/x86_64
    try_build $DOCKER_DEFAULT_PLATFORM
  else
    try_build $DOCKER_DEFAULT_PLATFORM
  fi
else
  try_build $DOCKER_DEFAULT_PLATFORM
fi

docker tag ${GITUSER}/${GITREPO}:latest ${GITUSER}/${GITREPO}:v${VERSION}

# Show the list of available images for this repo
echo "Image is ready"
docker images | grep ${GITREPO}

if [ $? -ne 0 ]; then
  kill "$PPID"; exit 1;
fi
CONTAINER_ID=$(docker ps -n=1 -q)
printf "\nFinished building Docker container ${CONTAINER_ID}.\n\n"

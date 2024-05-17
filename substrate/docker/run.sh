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

ARGS=$@

try_run() {
  PLATFORM=$1
  printf "Running Substrate Docker container for $PLATFORM architecture with provided arguments: $ARGS\n\n"

  docker run \
    --platform $PLATFORM \
    --hostname parity-substrate \
    --name parity-substrate \
    --memory 750M \
    --memory-reservation 125M \
    --memory-swap 15G \
    --cpus 1 \
    --publish 0.0.0.0:30333:30333 \
    --publish 0.0.0.0:9933:9933 \
    --publish 0.0.0.0:9944:9944 \
    --publish 0.0.0.0:9615:9615 \
    --publish 0.0.0.0:443:443 \
    --publish 0.0.0.0:80:80 \
    --rm -it parity/substrate $ARGS
}

# handle when arguments not provided. run arguments provided to script.
if [ "$ARGS" = "" ] ; then
  printf "Note: Please try providing an argument to the script.\n\n"
  exit 1
else
  if [[ "$OSTYPE" == "darwin"* ]]; then
    set -e

    echo "Mac OS (Darwin) detected."
    if [[ $(uname -m) == 'arm64' ]]; then
      echo "Detected Apple Silicon"
      # emulate using `linux/x86_64` for Apple Silicon to avoid error with `/lib64/ld-linux-x86-64.so.2`
      DOCKER_DEFAULT_PLATFORM=linux/x86_64
      try_run $DOCKER_DEFAULT_PLATFORM
    else
      try_run $DOCKER_DEFAULT_PLATFORM
    fi
  else
    try_run $DOCKER_DEFAULT_PLATFORM
  fi
fi

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

  # chain syncing will utilize all available memory and CPU power
  # as mentioned in /substrate/primitives/runtime/docs/contributor/docker.md
  # so configure `--memory`, `--memory-reservation`, `--memory-swap`, and `--cpus` values below
  # to limit resources used.
  #
  # to run the commands within the Docker container itself instead of from the host machine
  # run the Docker container in the background in detached mode with
  # `-d` (e.g. `docker run --platform $PLATFORM -it -d parity/substrate`) and then enter that
  # docker container with `docker exec -it parity/substrate /bin/bash`
  #
  # in addition to exposing ports in the Dockerfile using `EXPOSE` to open ports on the container side,
  # it is also necessary to publish the ports to open them to the outside world on the Docker host side.
  # additional ports 4433 and 80 have been exposed and published incase the user wishes to run a
  # frontend from within the Docker container.
  #
  # if you want to restart on failure use `--restart "on-failure"` with `--rm`
  docker run \
    --platform $PLATFORM \
    --hostname substrate \
    --name substrate \
    --memory 750M \
    --memory-reservation 125M \
    --memory-swap 15G \
    --cpus 1 \
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

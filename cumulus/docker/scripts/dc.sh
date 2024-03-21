#!/usr/bin/env bash
# helper function to run docker-compose using the docker/docker-compose.yml file while
# retaining a context from the root of the repository

set -e

dc () {
    cd "$(cd "$(dirname "$0")" && git rev-parse --show-toplevel)"
    docker-compose -f - "$@" < docker/docker-compose.yml
}
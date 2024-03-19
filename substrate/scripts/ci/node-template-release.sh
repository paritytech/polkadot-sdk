#!/usr/bin/env bash

set -e

export TERM=xterm
SUBSTRATE_FOLDER="/substrate"
GIT_ROOT=`git rev-parse --show-toplevel`
PROJECT_ROOT=${GIT_ROOT}${SUBSTRATE_FOLDER}

if [ "$#" -ne 1 ]; then
  echo "node-template-release.sh path_to_target_archive"
  exit 1
fi

PATH_TO_ARCHIVE=$1

cd $PROJECT_ROOT/scripts/ci/node-template-release
cargo run $PROJECT_ROOT/bin/node-template $PROJECT_ROOT/$PATH_TO_ARCHIVE

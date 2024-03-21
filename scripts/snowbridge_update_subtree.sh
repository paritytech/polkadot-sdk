#!/bin/bash

# A script to udpate bridges repo as subtree to Cumulus
# Usage:
#       ./scripts/update_subtree_snowbridge.sh fetch
#       ./scripts/update_subtree_snowbridge.sh patch

set -e

SNOWBRIDGE_BRANCH="${SNOWBRIDGE_BRANCH:-main}"
POLKADOT_SDK_BRANCH="${POLKADOT_SDK_BRANCH:-master}"
SNOWBRIDGE_TARGET_DIR="${TARGET_DIR:-bridges/snowbridge}"

function fetch() {
    # the script is able to work only on clean git copy
    [[ -z "$(git status --porcelain)" ]] || {
        echo >&2 "The git copy must be clean (stash all your changes):";
        git status --porcelain
        exit 1;
    }

    local snowbridge_remote=$(git remote -v | grep "snowbridge.git (fetch)" | head -n1 | awk '{print $1;}')
    if [ -z "$snowbridge_remote" ]; then
        echo "Adding new remote: 'snowbridge' repo..."
        git remote add -f snowbridge https://github.com/Snowfork/snowbridge.git
        snowbridge_remote="snowbridge"
    else
        echo "Fetching remote: '${snowbridge_remote}' repo..."
        git fetch https://github.com/Snowfork/snowbridge.git --prune
    fi

    echo "Syncing/updating subtree with remote branch '${snowbridge_remote}/$SNOWBRIDGE_BRANCH' to target directory: '$SNOWBRIDGE_TARGET_DIR'"
    git subtree pull --prefix=$SNOWBRIDGE_TARGET_DIR ${snowbridge_remote} $SNOWBRIDGE_BRANCH --squash
}

function clean() {
    echo "Patching/removing unneeded stuff from subtree in target directory: '$SNOWBRIDGE_TARGET_DIR'"
    chmod +x $SNOWBRIDGE_TARGET_DIR/parachain/scripts/verify-pallets-build.sh
    $SNOWBRIDGE_TARGET_DIR/parachain/scripts/verify-pallets-build.sh --ignore-git-state --no-revert
}

function create_patch() {
    [[ -z "$(git status --porcelain)" ]] || {
        echo >&2 "The git copy must be clean (stash all your changes):";
        git status --porcelain
        exit 1;
    }
    echo "Creating diff patch file to apply to snowbridge. No Cargo.toml files will be included in the patch."
    git diff snowbridge/$SNOWBRIDGE_BRANCH $POLKADOT_SDK_BRANCH:bridges/snowbridge --diff-filter=ACM -- . ':(exclude)*/Cargo.toml' > snowbridge.patch
}

case "$1" in
    fetch)
        fetch
        ;;
    clean)
        clean
        ;;
    create_patch)
        create_patch
        ;;
    update)
        fetch
        clean
        ;;
esac

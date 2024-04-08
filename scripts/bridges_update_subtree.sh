#!/bin/bash

# A script to update bridges repo as subtree to Cumulus
# Usage:
#       ./scripts/bridges_update_subtree.sh fetch
#       ./scripts/bridges_update_subtree.sh patch
#       ./scripts/bridges_update_subtree.sh merge

set -e

BRIDGES_BRANCH="${BRANCH:-polkadot-staging}"
BRIDGES_TARGET_DIR="${TARGET_DIR:-bridges}"

function fetch() {
    # the script is able to work only on clean git copy
    [[ -z "$(git status --porcelain)" ]] || {
        echo >&2 "The git copy must be clean (stash all your changes):";
        git status --porcelain
        exit 1;
    }

    local bridges_remote=$(git remote -v | grep "parity-bridges-common.git (fetch)" | head -n1 | awk '{print $1;}')
    if [ -z "$bridges_remote" ]; then
        echo ""
        echo "Adding new remote: 'bridges' repo..."
        echo ""
        echo "... check your YubiKey ..."
        git remote add -f bridges git@github.com:paritytech/parity-bridges-common.git
        bridges_remote="bridges"
    else
        echo ""
        echo "Fetching remote: '${bridges_remote}' repo..."
        echo ""
        echo "... check your YubiKey ..."
        git fetch ${bridges_remote} --prune
    fi

    echo ""
    echo "Syncing/updating subtree with remote branch '${bridges_remote}/$BRIDGES_BRANCH' to target directory: '$BRIDGES_TARGET_DIR'"
    echo ""
    echo "... check your YubiKey ..."
    git subtree pull --prefix=$BRIDGES_TARGET_DIR ${bridges_remote} $BRIDGES_BRANCH --squash
}

function patch() {
    echo ""
    echo "Patching/removing unneeded stuff from subtree in target directory: '$BRIDGES_TARGET_DIR'"
    $BRIDGES_TARGET_DIR/scripts/verify-pallets-build.sh --ignore-git-state --no-revert
}

function merge() {
    echo ""
    echo "Merging stuff from subtree in target directory: '$BRIDGES_TARGET_DIR'"

    # stage all removed by patch: DU, MD, D, AD - only from subtree directory
    git status -s | awk '$1 == "DU" || $1 == "D" || $1 == "MD" || $1 == "AD" {print $2}' | grep "^$BRIDGES_TARGET_DIR/" | xargs git rm -q --ignore-unmatch

    echo ""
    echo "When all conflicts are resolved, do 'git merge --continue'"
}

function amend() {
    echo ""
    echo "Amend stuff from subtree in target directory: '$BRIDGES_TARGET_DIR'"
    git commit --amend -S -m "updating bridges subtree + remove extra folders"
}

case "$1" in
    fetch)
        fetch
        ;;
    patch)
        patch
        ;;
    merge)
        merge
        ;;
    amend)
        amend
        ;;
    all)
        fetch
        patch
        ;;
esac

#!/bin/bash

# A script to udpate bridges repo as subtree to Cumulus
# Usage:
#       ./scripts/update_subtree_snowbridge.sh fetch
#       ./scripts/update_subtree_snowbridge.sh patch
#       ./scripts/update_subtree_snowbridge.sh merge

set -e

SNOWBRIDGE_BRANCH="${BRANCH:-main}"
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
        echo ""
        echo "Adding new remote: 'snowbridge' repo..."
        echo ""
        git remote add -f snowbridge git@github.com:snowfork/snowbridge.git
        snowbridge_remote="snowbridge"
    else
        echo ""
        echo "Fetching remote: '${snowbridge_remote}' repo..."
        echo ""
        git fetch ${snowbridge_remote} --prune
    fi

    echo ""
    echo "Syncing/updating subtree with remote branch '${snowbridge_remote}/$SNOWBRIDGE_BRANCH' to target directory: '$SNOWBRIDGE_TARGET_DIR'"
    echo ""
    git subtree pull --prefix=$SNOWBRIDGE_TARGET_DIR ${snowbridge_remote} $SNOWBRIDGE_BRANCH --squash
}

function patch() {
    echo ""
    echo "Patching/removing unneeded stuff from subtree in target directory: '$$SNOWBRIDGE_TARGET_DIR'"
    $SNOWBRIDGE_TARGET_DIR/scripts/verify-pallets-build.sh --ignore-git-state --no-revert
}

function merge() {
    echo ""
    echo "Merging stuff from subtree in target directory: '$SNOWBRIDGE_TARGET_DIR'"

    # stage all removed by patch: DU, MD, D, AD - only from subtree directory
    git status -s | awk '$1 == "DU" || $1 == "D" || $1 == "MD" || $1 == "AD" {print $2}' | grep "^$SNOWBRIDGE_TARGET_DIR/" | xargs git rm -q --ignore-unmatch

    echo ""
    echo "When all conflicts are resolved, do 'git merge --continue'"
}

function amend() {
    echo ""
    echo "Amend stuff from subtree in target directory: '$SNOWBRIDGE_TARGET_DIR'"
    git commit --amend -S -m "updating snowbridge subtree + remove extra folders"
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

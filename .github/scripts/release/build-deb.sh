#!/usr/bin/env bash
set -e

PRODUCT=$1
VERSION=$2
PROFILE=${PROFILE:-production}

cargo install --version 2.7.0 cargo-deb --locked -q
echo "Using cargo-deb v$(cargo-deb --version)"
echo "Building a Debian package for '$PRODUCT' in '$PROFILE' profile"

# we need to start  the custom version with a didgit as requires it cargo-deb
cargo deb --profile $PROFILE --no-strip --no-build -p $PRODUCT --deb-version 1-$VERSION

deb=target/debian/$PRODUCT_*_amd64.deb

cp $deb target/production/

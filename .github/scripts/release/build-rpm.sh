#!/usr/bin/env bash
set -e

PRODUCT=$1
VERSION=$2
PROFILE=${PROFILE:-production}

# Install cargo-rpm. A specific version can be used if needed.
cargo install cargo-rpm --locked -q
echo "Using cargo-rpm v$(cargo rpm --version)"
echo "Building an RPM package for '$PRODUCT' in '$PROFILE' profile"


cargo rpm build --release -p $PRODUCT


rpm_file=target/x86_64/rpmbuild/RPMS/x86_64/$PRODUCT-*-1.x86_64.rpm

# Check if the file exists before attempting to copy.
if [ ! -f "$rpm_file" ]; then
    echo "Error: RPM package file not found at expected path."
    exit 1
fi

# The artifacts are copied to a designated `target/production` directory.
mkdir -p target/production
cp $rpm_file target/production/

echo "RPM package build complete. Artifact copied to target/production/."
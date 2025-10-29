#!/usr/bin/env bash
set -e

# --- Configuration ---
PRODUCT=${1:?"Usage: $0 <product_name> <version>"}
VERSION=${2:?"Usage: $0 <product_name> <version>"}
PROFILE=${PROFILE:-production}
ARCH="x86_64"

SOURCE_DIR="target/${PROFILE}"
STAGING_DIR="/tmp/${PRODUCT}-staging"
DEST_DIR="target/production"

# --- Script Start ---
echo "📦 Starting RPM build for '$PRODUCT' version '$VERSION'..."

# 1. Clean up and create a fresh staging directory
echo "🔧 Setting up staging directory: ${STAGING_DIR}"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/lib/${PRODUCT}"
mkdir -p "$STAGING_DIR/usr/lib/systemd/system"
mkdir -p "$STAGING_DIR/etc/default"

# 2. Copy compiled binaries and assets into the staging directory
echo "📂 Copying application files..."
cp "${SOURCE_DIR}/${PRODUCT}" "${STAGING_DIR}/usr/bin/"
cp "${SOURCE_DIR}/${PRODUCT}-prepare-worker" "${STAGING_DIR}/usr/lib/${PRODUCT}/"
cp "${SOURCE_DIR}/${PRODUCT}-execute-worker" "${STAGING_DIR}/usr/lib/${PRODUCT}/"
# MODIFIED PATH: Prefixed with the subdirectory name
cp "polkadot/scripts/packaging/polkadot.service" "${STAGING_DIR}/usr/lib/systemd/system/"

# 3. Use fpm to package the staging directory into an RPM
# fpm config file .fpm is located in the polkadot-sdk root directory
echo "🎁 Running fpm to create the RPM package..."
fpm \
  -s dir \
  -t rpm \
  -n "$PRODUCT" \
  -v "$VERSION" \
  -a "$ARCH" \
  --rpm-os linux \
  --description "Polkadot Node" \
  --license "GPL-3.0-only" \
  --url "https://polkadot.network/" \
  --depends systemd \
  --depends shadow-utils \
  --after-install "polkadot/scripts/packaging/rpm-maintainer-scripts/rpm-postinst.sh" \
  --before-remove "polkadot/scripts/packaging/rpm-maintainer-scripts/rpm-preun.sh" \
  --after-remove "polkadot/scripts/packaging/rpm-maintainer-scripts/rpm-postun.sh" \
  --config-files "/etc/default/polkadot" \
  -C "$STAGING_DIR" \
  .

# 4. Move the final RPM to the artifacts directory
echo "🚚 Moving RPM to '${DEST_DIR}'..."
mkdir -p "$DEST_DIR"
mv "${PRODUCT}-${VERSION}-1.${ARCH}.rpm" "$DEST_DIR/"

# 5. Clean up the staging directory
echo "🧹 Cleaning up temporary files..."
rm -rf "$STAGING_DIR"

echo "✅ RPM package build complete!"
ls -l "$DEST_DIR"

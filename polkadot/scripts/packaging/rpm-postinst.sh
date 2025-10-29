#!/bin/sh
# Post-install script for RPM package

set -e

config_file="/etc/default/polkadot"

# Create polkadot group if it doesn't exist
getent group polkadot >/dev/null || groupadd -r polkadot

# Create polkadot user if it doesn't exist
getent passwd polkadot >/dev/null || \
    useradd -r -g polkadot -d /home/polkadot -m -s /sbin/nologin \
    -c "User account for running polkadot as a service" polkadot

# Create default config file if it doesn't exist
if [ ! -e "$config_file" ]; then
    echo 'POLKADOT_CLI_ARGS=""' > "$config_file"
fi

# Reload systemd daemon to recognize the new service
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
fi

exit 0

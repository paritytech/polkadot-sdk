#!/bin/sh
# Post-uninstall script for RPM package

set -e

# Reload systemd after service file removal (but not on upgrade)
if [ "$1" = "0" ]; then
    # $1 = 0 means uninstall (not upgrade)
    if command -v systemctl >/dev/null 2>&1; then
        systemctl daemon-reload || true
    fi
fi

exit 0

#!/usr/bin/env bash
set -eu

source scripts/set-env.sh
source scripts/xcm-helper.sh

config_xcm_version() {
    local call="0x1f04020109079edaa80203000000"
    send_governance_transact_from_relaychain $ASSET_HUB_PARAID "$call"
}

configure_assethub() {
    config_xcm_version
}

if [ -z "${from_start_services:-}" ]; then
    echo "config assethub only!"
    configure_assethub
    wait
fi

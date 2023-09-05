#!/bin/bash

# THIS SCRIPT IS NOT INTENDED FOR USE IN PRODUCTION ENVIRONMENT
#
# This scripts periodically calls the Substrate relay binary to generate messages. These messages
# are sent from the Rialto network to the Millau network.

set -eu

# Max delay before submitting transactions (s)
MAX_SUBMIT_DELAY_S=${MSG_EXCHANGE_GEN_MAX_SUBMIT_DELAY_S:-30}
MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE=1024

SHARED_CMD="/home/user/substrate-relay send-message rialto-to-millau"
SHARED_HOST="--source-host rialto-node-bob --source-port 9944"
SOURCE_SIGNER="--source-signer //Millau.MessagesSender"

SEND_MESSAGE="$SHARED_CMD $SHARED_HOST $SOURCE_SIGNER"

SOURCE_CHAIN="Rialto"
TARGET_CHAIN="Millau"
EXTRA_ARGS=""
# It is the encoded `xcm::VersionedXcm::V3(prepare_outbound_xcm_message(MillauNetwork::get())`
# from the `xcm_messages_to_millau_are_sent_using_bridge_exporter` test in the `rialto-runtime`
REGULAR_PAYLOAD="030426030109030419a8"

source /common/generate_messages.sh

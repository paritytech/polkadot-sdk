#!/bin/bash

# Script for generating messages from a source chain to a target chain.
# Prerequisites: mounting the common folder in the docker container (Adding the following volume entry):
# - ./bridges/common:/common
# It can be used by executing `source /common/generate_messages.sh` in a different script,
# after setting the following variables:
# SOURCE_CHAIN
# TARGET_CHAIN
# MAX_SUBMIT_DELAY_S
# SEND_MESSAGE - the command that is executed to send a message
# SECONDARY_EXTRA_ARGS - optional, for example "--use-xcm-pallet"
# EXTRA_ARGS - for example "--use-xcm-pallet"
# REGULAR_PAYLOAD
# MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE

SECONDARY_EXTRA_ARGS=${SECONDARY_EXTRA_ARGS:-""}

trap "echo Exiting... TERM; exit $?" TERM

# Sleep a bit between messages
rand_sleep() {
	SUBMIT_DELAY_S=`shuf -i 0-$MAX_SUBMIT_DELAY_S -n 1`
	echo "Sleeping $SUBMIT_DELAY_S seconds..."
	sleep $SUBMIT_DELAY_S & wait $!
	NOW=`date "+%Y-%m-%d %H:%M:%S"`
	echo "Woke up at $NOW"
}

# start sending large messages immediately
LARGE_MESSAGES_TIME=0
# start sending message packs in a hour
BUNCH_OF_MESSAGES_TIME=3600

while true
do
	rand_sleep

	# send regular message
	echo "Sending Message from $SOURCE_CHAIN to $TARGET_CHAIN"
	$SEND_MESSAGE $EXTRA_ARGS raw $REGULAR_PAYLOAD

	# every other hour we're sending 3 large (size, weight, size+weight) messages
	if [ $SECONDS -ge $LARGE_MESSAGES_TIME ]; then
		LARGE_MESSAGES_TIME=$((SECONDS + 7200))

		rand_sleep
		echo "Sending Maximal Size Message from $SOURCE_CHAIN to $TARGET_CHAIN"
		$SEND_MESSAGE \
			sized max
	fi

	# every other hour we're sending a bunch of small messages
	if [ $SECONDS -ge $BUNCH_OF_MESSAGES_TIME ]; then
		BUNCH_OF_MESSAGES_TIME=$((SECONDS + 7200))

		for i in $(seq 0 $MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE);
		do
			$SEND_MESSAGE \
				$EXTRA_ARGS \
				raw $REGULAR_PAYLOAD
		done

	fi
done

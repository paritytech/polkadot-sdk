#!/usr/bin/env bash

set -e

head () {
    polkadot-js-api --ws ws://172.28.1.1:9944 query.parachains.heads 100 |\
        jq -r .heads
}

start=$(head)
sleep 60
end=$(head)

[ "$start" != "$end" ]

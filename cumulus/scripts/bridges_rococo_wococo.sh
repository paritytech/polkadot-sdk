#!/bin/bash

# Address: 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
# AccountId: [212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130, 44, 133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125]
STATEMINE_ACCOUNT_SEED_FOR_LOCAL="//Alice"
# Address: 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
# AccountId: [212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130, 44, 133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125]
WOCKMINT_ACCOUNT_ADDRESS_FOR_LOCAL="5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"

# Address: GegTpZJMyzkntLN7NJhRfHDk4GWukLbGSsag6PHrLSrCK4h
ROCKMINE2_ACCOUNT_SEED_FOR_ROCOCO="scatter feed race company oxygen trip extra elbow slot bundle auto canoe"

# Adress: 5Ge7YcbctWCP1CccugzxWDn9hFnTxvTh3bL6PNy4ubNJmp7Y / H9jCvwVWsDJkrS4gPp1QB99qr4hmbGsVyAqn3F2PPaoWyU3
# AccountId: [202, 107, 198, 135, 15, 25, 193, 165, 172, 73, 137, 218, 115, 177, 204, 0, 5, 155, 215, 86, 208, 51, 50, 130, 190, 110, 184, 143, 124, 50, 160, 20]
WOCKMINT_ACCOUNT_ADDRESS_FOR_ROCOCO="5Ge7YcbctWCP1CccugzxWDn9hFnTxvTh3bL6PNy4ubNJmp7Y"
WOCKMINT_ACCOUNT_SEED_FOR_WOCOCO="tone spirit magnet sunset cannon poverty forget lock river east blouse random"

function address_to_account_id_bytes() {
    local address=$1
    local output=$2
    echo "address_to_account_id_bytes - address: $address, output: $output"
    if [ $address == "$WOCKMINT_ACCOUNT_ADDRESS_FOR_LOCAL" ]; then
        jq --null-input '[212, 53, 147, 199, 21, 253, 211, 28, 97, 20, 26, 189, 4, 169, 159, 214, 130, 44, 133, 88, 133, 76, 205, 227, 154, 86, 132, 231, 165, 109, 162, 125]' > $output
    elif [ $address == "$WOCKMINT_ACCOUNT_ADDRESS_FOR_ROCOCO" ]; then
        jq --null-input '[202, 107, 198, 135, 15, 25, 193, 165, 172, 73, 137, 218, 115, 177, 204, 0, 5, 155, 215, 86, 208, 51, 50, 130, 190, 110, 184, 143, 124, 50, 160, 20]' > $output
    else
        echo -n "Sorry, unknown address: $address - please, add bytes here or function for that!"
        exit 1
    fi
}

function ensure_binaries() {
    if [[ ! -f ~/local_bridge_testing/bin/polkadot ]]; then
        echo "  Required polkadot binary '~/local_bridge_testing/bin/polkadot' does not exist!"
        echo "  You need to build it and copy to this location!"
        echo "  Please, check ./parachains/runtimes/bridge-hubs/README.md (Prepare/Build/Deploy)"
        exit 1
    fi
    if [[ ! -f ~/local_bridge_testing/bin/polkadot-parachain ]]; then
        echo "  Required polkadot-parachain binary '~/local_bridge_testing/bin/polkadot-parachain' does not exist!"
        echo "  You need to build it and copy to this location!"
        echo "  Please, check ./parachains/runtimes/bridge-hubs/README.md (Prepare/Build/Deploy)"
        exit 1
    fi
}

function ensure_relayer() {
    if [[ ! -f ~/local_bridge_testing/bin/substrate-relay ]]; then
        echo "  Required substrate-relay binary '~/local_bridge_testing/bin/substrate-relay' does not exist!"
        echo "  You need to build it and copy to this location!"
        echo "  Please, check ./parachains/runtimes/bridge-hubs/README.md (Prepare/Build/Deploy)"
        exit 1
    fi
}

function ensure_polkadot_js_api() {
    if ! which polkadot-js-api &> /dev/null; then
        echo ''
        echo 'Required command `polkadot-js-api` not in PATH, please, install, e.g.:'
        echo "npm install -g @polkadot/api-cli@beta"
        echo "      or"
        echo "yarn global add @polkadot/api-cli"
        echo ''
        exit 1
    fi
    if ! which jq &> /dev/null; then
        echo ''
        echo 'Required command `jq` not in PATH, please, install, e.g.:'
        echo "apt install -y jq"
        echo ''
        exit 1
    fi
    generate_hex_encoded_call_data "check" "--"
    local retVal=$?
    if [ $retVal -ne 0 ]; then
        echo ""
        echo ""
        echo "-------------------"
        echo "Installing (nodejs) sub module: ./scripts/generate_hex_encoded_call"
        pushd ./scripts/generate_hex_encoded_call
        npm install
        popd
    fi
}

function generate_hex_encoded_call_data() {
    local type=$1
    local endpoint=$2
    local output=$3
    shift
    shift
    shift
    echo "Input params: $@"

    node ./scripts/generate_hex_encoded_call "$type" "$endpoint" "$output" "$@"
    local retVal=$?

    if [ $type != "check" ]; then
        local hex_encoded_data=$(cat $output)
        echo "Generated hex-encoded bytes to file '$output': $hex_encoded_data"
    fi

    return $retVal
}

function transfer_balance() {
    local runtime_para_endpoint=$1
    local seed=$2
    local target_account=$3
    local amount=$4
    echo "  calling transfer_balance:"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      seed: ${seed}"
    echo "      target_account: ${target_account}"
    echo "      amount: ${amount}"
    echo "--------------------------------------------------"

    polkadot-js-api \
        --ws "${runtime_para_endpoint}" \
        --seed "${seed?}" \
        tx.balances.transfer \
            "${target_account}" \
            "${amount}"
}

function send_governance_transact() {
    local relay_url=$1
    local relay_chain_seed=$2
    local para_id=$3
    local hex_encoded_data=$4
    local require_weight_at_most_ref_time=$5
    local require_weight_at_most_proof_size=$6
    echo "  calling send_governance_transact:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      para_id: ${para_id}"
    echo "      hex_encoded_data: ${hex_encoded_data}"
    echo "      require_weight_at_most_ref_time: ${require_weight_at_most_ref_time}"
    echo "      require_weight_at_most_proof_size: ${require_weight_at_most_proof_size}"
    echo "      params:"

    local dest=$(jq --null-input \
                    --arg para_id "$para_id" \
                    '{ "V3": { "parents": 0, "interior": { "X1": { "Parachain": $para_id } } } }')

    local message=$(jq --null-input \
                       --argjson hex_encoded_data $hex_encoded_data \
                       --arg require_weight_at_most_ref_time "$require_weight_at_most_ref_time" \
                       --arg require_weight_at_most_proof_size "$require_weight_at_most_proof_size" \
                       '
                       {
                          "V3": [
                                  {
                                    "UnpaidExecution": {
                                        "weight_limit": "Unlimited"
                                    }
                                  },
                                  {
                                    "Transact": {
                                      "origin_kind": "Superuser",
                                      "require_weight_at_most": {
                                        "ref_time": $require_weight_at_most_ref_time,
                                        "proof_size": $require_weight_at_most_proof_size,
                                      },
                                      "call": {
                                        "encoded": $hex_encoded_data
                                      }
                                    }
                                  }
                          ]
                        }
                        ')

    echo ""
    echo "          dest:"
    echo "${dest}"
    echo ""
    echo "          message:"
    echo "${message}"
    echo ""
    echo "--------------------------------------------------"

    polkadot-js-api \
        --ws "${relay_url?}" \
        --seed "${relay_chain_seed?}" \
        --sudo \
        tx.xcmPallet.send \
            "${dest}" \
            "${message}"
}

function allow_assets_transfer_send() {
    local relay_url=$1
    local relay_chain_seed=$2
    local runtime_para_id=$3
    local runtime_para_endpoint=$4
    local bridge_hub_para_id=$5
    local bridged_para_network=$6
    local bridged_para_para_id=$7
    echo "  calling allow_assets_transfer_send:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      runtime_para_id: ${runtime_para_id}"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      bridge_hub_para_id: ${bridge_hub_para_id}"
    echo "      bridged_para_network: ${bridged_para_network}"
    echo "      bridged_para_para_id: ${bridged_para_para_id}"
    echo "      params:"

    # 1. generate data for Transact (add_exporter_config)
    local bridge_config=$(jq --null-input \
                             --arg bridge_hub_para_id "$bridge_hub_para_id" \
                             --arg bridged_para_network "$bridged_para_network" \
                             --arg bridged_para_para_id "$bridged_para_para_id" \
        '
            {
                "bridgeLocation": {
                    "parents": 1,
                    "interior": {
                        "X1": { "Parachain": $bridge_hub_para_id }
                    }
                },
                "allowedTargetLocation": {
                    "parents": 2,
                    "interior": {
                        "X2": [
                            {
                                "GlobalConsensus": $bridged_para_network,
                            },
                            {
                                "Parachain": $bridged_para_para_id
                            }
                        ]
                    }
                },
                "maxTargetLocationFee": {
                    "id": {
                        "Concrete": {
                            "parents": 1,
                            "interior": "Here"
                        }
                    },
                    "fun": {
                        "Fungible": 50000000000
                    }
                }
            }
        '
    )
    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "add-exporter-config" "${runtime_para_endpoint}" "${tmp_output_file}" $bridged_para_network "$bridge_config"
    local hex_encoded_data=$(cat $tmp_output_file)

    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000
}

function force_create_foreign_asset() {
    local relay_url=$1
    local relay_chain_seed=$2
    local runtime_para_id=$3
    local runtime_para_endpoint=$4
    local global_consensus=$5
    local asset_owner_account_id=$6
    echo "  calling force_create_foreign_asset:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      runtime_para_id: ${runtime_para_id}"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      global_consensus: ${global_consensus}"
    echo "      asset_owner_account_id: ${asset_owner_account_id}"
    echo "      params:"

    # 1. generate data for Transact (ForeignAssets::force_create)
    local asset_id=$(jq --null-input \
                             --arg global_consensus "$global_consensus" \
        '
            {
                "parents": 2,
                "interior": {
                    "X1": {
                        "GlobalConsensus": $global_consensus,
                    }
                }
            }
        '
    )
    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "force-create-asset" "${runtime_para_endpoint}" "${tmp_output_file}" "$asset_id" "$asset_owner_account_id" false "1000"
    local hex_encoded_data=$(cat $tmp_output_file)

    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000
}

function allow_assets_transfer_receive() {
    local relay_url=$1
    local relay_chain_seed=$2
    local runtime_para_id=$3
    local runtime_para_endpoint=$4
    local bridge_hub_para_id=$5
    local bridged_network=$6
    local bridged_para_id=$7
    echo "  calling allow_assets_transfer_receive:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      runtime_para_id: ${runtime_para_id}"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      bridge_hub_para_id: ${bridge_hub_para_id}"
    echo "      bridged_network: ${bridged_network}"
    echo "      bridged_para_id: ${bridged_para_id}"
    echo "      params:"

    # 1. generate data for Transact (add_universal_alias)
    local location=$(jq --null-input \
                        --arg bridge_hub_para_id "$bridge_hub_para_id" \
                        '{ "V3": { "parents": 1, "interior": { "X1": { "Parachain": $bridge_hub_para_id } } } }')

    local junction=$(jq --null-input \
                        --arg bridged_network "$bridged_network" \
                        '{ "GlobalConsensus": $bridged_network } ')

    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "add-universal-alias" "${runtime_para_endpoint}" "${tmp_output_file}" "$location" "$junction"
    local hex_encoded_data=$(cat $tmp_output_file)

    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000

    # 2. generate data for Transact (add_reserve_location)
    local reserve_location=$(jq --null-input \
                        --arg bridged_network "$bridged_network" \
                        --arg bridged_para_id "$bridged_para_id" \
                        '{ "V3": {
                            "parents": 2,
                            "interior": {
                                "X2": [
                                    {
                                        "GlobalConsensus": $bridged_network,
                                    },
                                    {
                                        "Parachain": $bridged_para_id
                                    }
                                ]
                            }
                        } }')

    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "add-reserve-location" "${runtime_para_endpoint}" "${tmp_output_file}" "$reserve_location"
    local hex_encoded_data=$(cat $tmp_output_file)

    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000
}

function remove_assets_transfer_send() {
    local relay_url=$1
    local relay_chain_seed=$2
    local runtime_para_id=$3
    local runtime_para_endpoint=$4
    local bridged_network=$5
    echo "  calling remove_assets_transfer_send:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      runtime_para_id: ${runtime_para_id}"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      bridged_network: ${bridged_network}"
    echo "      params:"

    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "remove-exporter-config" "${runtime_para_endpoint}" "${tmp_output_file}" $bridged_network
    local hex_encoded_data=$(cat $tmp_output_file)

    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000
}

# TODO: we need to fill sovereign account for bridge-hub, because, small ammouts does not match ExistentialDeposit, so no reserve pass
# SA for BH: MultiLocation { parents: 1, interior: X1(Parachain(1013)) } - 5Eg2fntRRwLinojmk3sh5xscp7F3S6Zzm5oDVtoLTALKiypR on Statemine

function transfer_asset_via_bridge() {
    local url=$1
    local seed=$2
    local target_account=$3
    echo "  calling transfer_asset_via_bridge:"
    echo "      url: ${url}"
    echo "      seed: ${seed}"
    echo "      target_account: ${target_account}"
    echo "      params:"

    local assets=$(jq --null-input \
        '
        {
            "V3": [
                {
                    "id": {
                        "Concrete": {
                            "parents": 1,
                            "interior": "Here"
                        }
                    },
                    "fun": {
                        "Fungible": 100000000
                    }
                }
            ]
        }
        '
    )

    local tmp_output_file=$(mktemp)
    address_to_account_id_bytes "$target_account" "${tmp_output_file}"
    local hex_encoded_data=$(cat $tmp_output_file)

    local destination=$(jq --null-input \
                           --argjson hex_encoded_data "$hex_encoded_data" \
        '
            {
                "V3": {
                    "parents": 2,
                    "interior": {
                        "X3": [
                            {
                                "GlobalConsensus": "Wococo"
                            },
                            {
                                "Parachain": 1000
                            },
                            {
                                "AccountId32": {
                                    "id": $hex_encoded_data
                                }
                            }
                        ]
                    }
                }
            }
        '
    )

    echo ""
    echo "          assets:"
    echo "${assets}"
    echo ""
    echo "          destination:"
    echo "${destination}"
    echo ""
    echo "--------------------------------------------------"

    polkadot-js-api \
        --ws "${url?}" \
        --seed "${seed?}" \
        tx.bridgeTransfer.transferAssetViaBridge \
            "${assets}" \
            "${destination}"
}

function ping_via_bridge() {
    local url=$1
    local seed=$2
    local target_account=$3
    echo "  calling ping_via_bridge:"
    echo "      url: ${url}"
    echo "      seed: ${seed}"
    echo "      target_account: ${target_account}"
    echo "      params:"

    local tmp_output_file=$(mktemp)
    address_to_account_id_bytes "$target_account" "${tmp_output_file}"
    local hex_encoded_data=$(cat $tmp_output_file)

    local destination=$(jq --null-input \
                           --argjson hex_encoded_data "$hex_encoded_data" \
        '
            {
                "V3": {
                    "parents": 2,
                    "interior": {
                        "X3": [
                            {
                                "GlobalConsensus": "Wococo"
                            },
                            {
                                "Parachain": 1000
                            },
                            {
                                "AccountId32": {
                                    "id": $hex_encoded_data
                                }
                            }
                        ]
                    }
                }
            }
        '
    )

    echo ""
    echo "          destination:"
    echo "${destination}"
    echo ""
    echo "--------------------------------------------------"

    polkadot-js-api \
        --ws "${url?}" \
        --seed "${seed?}" \
        tx.bridgeTransfer.pingViaBridge \
            "${destination}"
}

function init_ro_wo() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay init-bridge rococo-to-bridge-hub-wococo \
	--source-host localhost \
	--source-port 9942 \
	--source-version-mode Auto \
	--target-host localhost \
	--target-port 8945 \
	--target-version-mode Auto \
	--target-signer //Bob
}

function init_wo_ro() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay init-bridge wococo-to-bridge-hub-rococo \
        --source-host localhost \
        --source-port 9945 \
        --source-version-mode Auto \
        --target-host localhost \
        --target-port 8943 \
        --target-version-mode Auto \
        --target-signer //Bob
}

function run_relay() {
    ensure_relayer

    RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
        ~/local_bridge_testing/bin/substrate-relay relay-headers-and-messages bridge-hub-rococo-bridge-hub-wococo \
        --rococo-host localhost \
        --rococo-port 9942 \
        --rococo-version-mode Auto \
        --bridge-hub-rococo-host localhost \
        --bridge-hub-rococo-port 8943 \
        --bridge-hub-rococo-version-mode Auto \
        --bridge-hub-rococo-signer //Charlie \
        --wococo-headers-to-bridge-hub-rococo-signer //Bob \
        --wococo-parachains-to-bridge-hub-rococo-signer //Bob \
        --bridge-hub-rococo-transactions-mortality 4 \
        --wococo-host localhost \
        --wococo-port 9945 \
        --wococo-version-mode Auto \
        --bridge-hub-wococo-host localhost \
        --bridge-hub-wococo-port 8945 \
        --bridge-hub-wococo-version-mode Auto \
        --bridge-hub-wococo-signer //Charlie \
        --rococo-headers-to-bridge-hub-wococo-signer //Bob \
        --rococo-parachains-to-bridge-hub-wococo-signer //Bob \
        --bridge-hub-wococo-transactions-mortality 4 \
        --lane 00000001
}

case "$1" in
  run-relay)
    init_ro_wo
    init_wo_ro
    run_relay
    ;;
  allow-transfers-local)
      # this allows send transfers on statemine (by governance-like)
      ./$0 "allow-transfer-on-statemine-local"
      # this allows receive transfers on westmint (by governance-like)
      ./$0 "allow-transfer-on-westmint-local"
      ;;
  allow-transfer-on-statemine-local)
      ensure_polkadot_js_api
      allow_assets_transfer_send \
          "ws://127.0.0.1:9942" \
          "//Alice" \
          1000 \
          "ws://127.0.0.1:9910" \
          1013 \
          "Wococo" 1000
      ;;
  allow-transfer-on-westmint-local)
      ensure_polkadot_js_api
      allow_assets_transfer_receive \
          "ws://127.0.0.1:9945" \
          "//Alice" \
          1000 \
          "ws://127.0.0.1:9010" \
          1014 \
          "Rococo" \
          1000
      # drip SovereignAccount for `MultiLocation { parents: 2, interior: X2(GlobalConsensus(Rococo), Parachain(1000)) }` => 5DHZvp523gmJWxg9UcLVbofyu5nZkPvATeP1ciYncpFpXtiG
      # drip SovereignAccount for `MultiLocation { parents: 2, interior: X2(GlobalConsensus(Rococo), Parachain(1015)) }` => 5FS75NFUdEYhWHuV3y3ncjSG4PFdHfC5X7V6SEzc3rnCciwb
      transfer_balance \
          "ws://127.0.0.1:9010" \
          "//Alice" \
          "5DHZvp523gmJWxg9UcLVbofyu5nZkPvATeP1ciYncpFpXtiG" \
          $((1000000000 + 50000000000 * 20)) # ExistentialDeposit + maxTargetLocationFee * 20
      # create foreign assets for native Statemine token (yes, Kusama, because we are using Statemine runtime on rococo)
      force_create_foreign_asset \
          "ws://127.0.0.1:9945" \
          "//Alice" \
          1000 \
          "ws://127.0.0.1:9010" \
          "Kusama" \
          "5DHZvp523gmJWxg9UcLVbofyu5nZkPvATeP1ciYncpFpXtiG"
      ;;
  remove-assets-transfer-from-statemine-local)
      ensure_polkadot_js_api
      remove_assets_transfer_send \
          "ws://127.0.0.1:9942" \
          "//Alice" \
          1000 \
          "ws://127.0.0.1:9910" \
          "Wococo"
      ;;
  transfer-asset-from-statemine-local)
      ensure_polkadot_js_api
      transfer_asset_via_bridge \
          "ws://127.0.0.1:9910" \
          "$STATEMINE_ACCOUNT_SEED_FOR_LOCAL" \
          "$WOCKMINT_ACCOUNT_ADDRESS_FOR_LOCAL"
      ;;
  transfer-asset-from-statemine-rococo)
      ensure_polkadot_js_api
      transfer_asset_via_bridge \
          "wss://ws-rococo-rockmine2-collator-node-0.parity-testnet.parity.io" \
          "$ROCKMINE2_ACCOUNT_SEED_FOR_ROCOCO" \
          "$WOCKMINT_ACCOUNT_ADDRESS_FOR_ROCOCO"
      ;;
  ping-via-bridge-from-statemine-local)
      ensure_polkadot_js_api
      ping_via_bridge \
          "ws://127.0.0.1:9910" \
          "$STATEMINE_ACCOUNT_SEED_FOR_LOCAL" \
          "$WOCKMINT_ACCOUNT_ADDRESS_FOR_LOCAL"
      ;;
  ping-via-bridge-from-statemine-rococo)
      ensure_polkadot_js_api
      ping_via_bridge \
          "wss://ws-rococo-rockmine2-collator-node-0.parity-testnet.parity.io" \
          "${ROCKMINE2_ACCOUNT_SEED_FOR_ROCOCO}" \
          "$WOCKMINT_ACCOUNT_ADDRESS_FOR_ROCOCO"
      ;;
  drip)
      transfer_balance \
          "ws://127.0.0.1:9010" \
          "//Alice" \
          "5DHZvp523gmJWxg9UcLVbofyu5nZkPvATeP1ciYncpFpXtiG" \
          $((1000000000 + 50000000000 * 20))
      ;;
  stop)
    pkill -f polkadot
    pkill -f parachain
    ;;
  *)
    echo "A command is require. Supported commands for:
    Local (zombienet) run:
          - run-relay
          - allow-transfers-local
              - allow-transfer-on-statemine-local
              - allow-transfer-on-westmint-local
              - remove-assets-transfer-from-statemine-local
          - transfer-asset-from-statemine-local
          - ping-via-bridge-from-statemine-local
    Live Rococo/Wococo run:
          - transfer-asset-from-statemine-rococo
          - ping-via-bridge-from-statemine-rococo";
    exit 1
    ;;
esac

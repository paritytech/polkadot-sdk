#!/bin/bash

function relayer_path() {
    local default_path=~/local_bridge_testing/bin/substrate-relay
    local path="${SUBSTRATE_RELAY_BINARY:-$default_path}"
    echo "$path"
}

function ensure_relayer() {
    local path=$(relayer_path)
    if [[ ! -f "$path" ]]; then
        echo "  Required substrate-relay binary '$path' does not exist!"
        echo "  You need to build it and copy to this location!"
        echo "  Please, check ./parachains/runtimes/bridge-hubs/README.md (Prepare/Build/Deploy)"
        exit 1
    fi

    echo $path
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
        echo "Installing (nodejs) sub module: $(dirname "$0")/generate_hex_encoded_call"
        pushd $(dirname "$0")/generate_hex_encoded_call
        npm install
        popd
    fi
}

function call_polkadot_js_api() {
    # --noWait: without that argument `polkadot-js-api` waits until transaction is included into the block.
    #           With it, it just submits it to the tx pool and exits.
    # --nonce -1: means to compute transaction nonce using `system_accountNextIndex` RPC, which includes all
    #             transaction that are in the tx pool.
    polkadot-js-api --noWait --nonce -1 "$@"
}

function generate_hex_encoded_call_data() {
    local type=$1
    local endpoint=$2
    local output=$3
    shift
    shift
    shift
    echo "Input params: $@"

    node $(dirname "$0")/generate_hex_encoded_call "$type" "$endpoint" "$output" "$@"
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

    call_polkadot_js_api \
        --ws "${runtime_para_endpoint}" \
        --seed "${seed?}" \
        tx.balances.transferAllowDeath \
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

    call_polkadot_js_api \
        --ws "${relay_url?}" \
        --seed "${relay_chain_seed?}" \
        --sudo \
        tx.xcmPallet.send \
            "${dest}" \
            "${message}"
}

function open_hrmp_channels() {
    local relay_url=$1
    local relay_chain_seed=$2
    local sender_para_id=$3
    local recipient_para_id=$4
    local max_capacity=$5
    local max_message_size=$6
    echo "  calling open_hrmp_channels:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      sender_para_id: ${sender_para_id}"
    echo "      recipient_para_id: ${recipient_para_id}"
    echo "      max_capacity: ${max_capacity}"
    echo "      max_message_size: ${max_message_size}"
    echo "      params:"
    echo "--------------------------------------------------"
    call_polkadot_js_api \
        --ws "${relay_url?}" \
        --seed "${relay_chain_seed?}" \
        --sudo \
        tx.hrmp.forceOpenHrmpChannel \
            ${sender_para_id} \
            ${recipient_para_id} \
            ${max_capacity} \
            ${max_message_size}
}

function force_xcm_version() {
    local relay_url=$1
    local relay_chain_seed=$2
    local runtime_para_id=$3
    local runtime_para_endpoint=$4
    local dest=$5
    local xcm_version=$6
    echo "  calling force_xcm_version:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      runtime_para_id: ${runtime_para_id}"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      dest: ${dest}"
    echo "      xcm_version: ${xcm_version}"
    echo "      params:"

    # 1. generate data for Transact (PolkadotXcm::force_xcm_version)
    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "force-xcm-version" "${runtime_para_endpoint}" "${tmp_output_file}" "$dest" "$xcm_version"
    local hex_encoded_data=$(cat $tmp_output_file)

    # 2. trigger governance call
    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000
}

function force_create_foreign_asset() {
    local relay_url=$1
    local relay_chain_seed=$2
    local runtime_para_id=$3
    local runtime_para_endpoint=$4
    local asset_multilocation=$5
    local asset_owner_account_id=$6
    local min_balance=$7
    local is_sufficient=$8
    echo "  calling force_create_foreign_asset:"
    echo "      relay_url: ${relay_url}"
    echo "      relay_chain_seed: ${relay_chain_seed}"
    echo "      runtime_para_id: ${runtime_para_id}"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      asset_multilocation: ${asset_multilocation}"
    echo "      asset_owner_account_id: ${asset_owner_account_id}"
    echo "      min_balance: ${min_balance}"
    echo "      is_sufficient: ${is_sufficient}"
    echo "      params:"

    # 1. generate data for Transact (ForeignAssets::force_create)
    local tmp_output_file=$(mktemp)
    generate_hex_encoded_call_data "force-create-asset" "${runtime_para_endpoint}" "${tmp_output_file}" "$asset_multilocation" "$asset_owner_account_id" $is_sufficient $min_balance
    local hex_encoded_data=$(cat $tmp_output_file)

    # 2. trigger governance call
    send_governance_transact "${relay_url}" "${relay_chain_seed}" "${runtime_para_id}" "${hex_encoded_data}" 200000000 12000
}

function limited_reserve_transfer_assets() {
    local url=$1
    local seed=$2
    local destination=$3
    local beneficiary=$4
    local assets=$5
    local fee_asset_item=$6
    local weight_limit=$7
    echo "  calling limited_reserve_transfer_assets:"
    echo "      url: ${url}"
    echo "      seed: ${seed}"
    echo "      destination: ${destination}"
    echo "      beneficiary: ${beneficiary}"
    echo "      assets: ${assets}"
    echo "      fee_asset_item: ${fee_asset_item}"
    echo "      weight_limit: ${weight_limit}"
    echo ""
    echo "--------------------------------------------------"

    call_polkadot_js_api \
        --ws "${url?}" \
        --seed "${seed?}" \
        tx.polkadotXcm.limitedReserveTransferAssets \
            "${destination}" \
            "${beneficiary}" \
            "${assets}" \
            "${fee_asset_item}" \
            "${weight_limit}"
}

function claim_rewards() {
    local runtime_para_endpoint=$1
    local seed=$2
    local lane_id=$3
    local bridged_chain_id=$4
    local owner=$5
    echo "  calling claim_rewards:"
    echo "      runtime_para_endpoint: ${runtime_para_endpoint}"
    echo "      seed: ${seed}"
    echo "      lane_id: ${lane_id}"
    echo "      bridged_chain_id: ${bridged_chain_id}"
    echo "      owner: ${owner}"
    echo ""

    local rewards_account_params=$(jq --null-input \
                                      --arg lane_id "$lane_id" \
                                      --arg bridged_chain_id "$bridged_chain_id" \
                                      --arg owner "$owner" \
                    '{
                        "laneId": $lane_id,
                        "bridgedChainId": $bridged_chain_id,
                        "owner": $owner
                     }')

    echo "          rewards_account_params:"
    echo "${rewards_account_params}"
    echo "--------------------------------------------------"

    call_polkadot_js_api \
        --ws "${runtime_para_endpoint}" \
        --seed "${seed?}" \
        tx.bridgeRelayers.claimRewards \
            "${rewards_account_params}"
}
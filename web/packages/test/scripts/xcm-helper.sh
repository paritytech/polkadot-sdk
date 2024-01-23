#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

send_governance_transact_from_relaychain() {
    local para_id=$1
    local hex_encoded_data=$2
    local require_weight_at_most_ref_time=${3:-2000000000}
    local require_weight_at_most_proof_size=${4:-12000}
    echo "  calling send_governance_transact:"
    echo "      relay_url: ${relaychain_ws_url}"
    echo "      relay_chain_seed: ${relaychain_sudo_seed}"
    echo "      para_id: ${para_id}"
    echo "      require_weight_at_most_ref_time: ${require_weight_at_most_ref_time}"
    echo "      require_weight_at_most_proof_size: ${require_weight_at_most_proof_size}"
    echo "      params:"

    local dest=$(jq --null-input \
                    --arg para_id "$para_id" \
                    '{ "v3": { "parents": 0, "interior": { "x1": { "parachain": $para_id } } } }')

    local message=$(jq --null-input \
                       --arg hex_encoded_data "$hex_encoded_data" \
                       --arg require_weight_at_most_ref_time "$require_weight_at_most_ref_time" \
                       --arg require_weight_at_most_proof_size "$require_weight_at_most_proof_size" \
                       '
                       {
                          "v3": [
                                  {
                                    "unpaidexecution": {
                                        "weight_limit": "unlimited"
                                    }
                                  },
                                  {
                                    "transact": {
                                      "origin_kind": "superuser",
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

    npx polkadot-js-api \
        --ws "${relaychain_ws_url?}" \
        --seed "${relaychain_sudo_seed?}" \
        --sudo \
        tx.xcmPallet.send \
            "${dest}" \
            "${message}"
}

transfer_balance() {
    local runtime_para_endpoint=$1
    local seed=$2
    local para_id=$3
    local amount=$4
    local target_account=$5

    local dest=$(jq --null-input \
                    --arg para_id "$para_id" \
                    '{ "v3": { "parents": 0, "interior": { "x1": { "parachain": $para_id } } } }')
    local beneficiary=$(jq --null-input \
                    --arg target_account "$target_account" \
                    '{ "v3": { "parents": 0, "interior": { "x1": { "accountid32": { "id": $target_account } } } } }')
    local assets=$(jq --null-input \
                    --arg amount "$amount" \
        '
        {
            "V3": [
                {
                    "id": {
                        "Concrete": {
                            "parents": 0,
                            "interior": "Here"
                        }
                    },
                    "fun": {
                        "Fungible": $amount
                    }
                }
            ]
        }
        '
    )
    local asset_fee_item=0

    echo "  calling transfer_balance:"
    echo "      target_account: ${target_account}"
    echo "      dest: ${dest}"
    echo "      beneficiary: ${beneficiary}"
    echo "      assets: ${assets}"
    echo "      asset_fee_item: ${asset_fee_item}"
    echo "--------------------------------------------------"

    npx polkadot-js-api \
        --ws "${runtime_para_endpoint}" \
        --seed "${seed?}" \
        tx.xcmPallet.teleportAssets \
            "${dest}" \
            "${beneficiary}" \
            "${assets}" \
            "${asset_fee_item}"
}

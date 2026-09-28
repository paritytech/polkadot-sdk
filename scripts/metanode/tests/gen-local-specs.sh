#!/usr/bin/env bash
# Generates the chain specs of the e2e network: a westend-local relay chain with Asset Hub and
# People registered at genesis, plus raw specs of both parachains, each with the given bootnodes.
#
#   gen-local-specs.sh <bin-dir> <out-dir> <relay-bootnodes> <asset-hub-bootnodes> <people-bootnodes>
#
# Bootnodes are comma-separated multiaddrs. Genesis parachains need no core setup: the paras
# pallet assigns a new core to each of them (polkadot/runtime/parachains/src/scheduler.rs).
set -euo pipefail

bin=${1:?bin dir} out=${2:?out dir}
relay_boot=${3:?relay bootnodes} ah_boot=${4:?asset-hub bootnodes} people_boot=${5:?people bootnodes}
# jq before 1.7 turns every number into a double, so the relay spec's u128 balances would come
# out as 1e+18 and the runtime would reject the genesis. e2e.sh runs this in the ubuntu-24.04 image.
case "$(jq --version)" in
  jq-1.[0-6] | jq-1.[0-6].*) echo "gen-local-specs.sh: needs jq >= 1.7, found $(jq --version)" >&2; exit 1 ;;
esac
mkdir -p "$out"

bootnodes() {
  jq -cn --arg list "$1" '$list | split(",") | map(select(. != ""))'
}

# The para id, as the collator sees it: ParachainInfo::ParachainId in the genesis storage (the
# spec's own para_id field is null for these chains). A little-endian u32.
para_id() {
  local value
  value=$(jq -r '.genesis.raw.top["0x0d715f2646c8f85767b5d2764bb2782604a74d81251e398fd8a0a4d55023bb3f"] // empty' "$1")
  [[ "$value" =~ ^0x[0-9a-f]{8}$ ]] || {
    echo "gen-local-specs.sh: no ParachainInfo::ParachainId in $1" >&2
    return 1
  }
  value=${value#0x}
  echo $((16#${value:6:2}${value:4:2}${value:2:2}${value:0:2}))
}

# Raw parachain specs first; the relay genesis must hold exactly their genesis head and code.
paras=()
for para in asset-hub:"$ah_boot" people:"$people_boot"; do
  name=${para%%:*}
  "$bin/polkadot-parachain" build-spec --chain "$name-westend-local" --raw --disable-default-bootnode |
    jq --argjson boot "$(bootnodes "${para#*:}")" '.bootNodes = $boot' >"$out/$name-westend-local.json"
  "$bin/polkadot-parachain" export-genesis-head --chain "$out/$name-westend-local.json" "$out/$name.head"
  "$bin/polkadot-parachain" export-genesis-wasm --chain "$out/$name-westend-local.json" "$out/$name.wasm"
  id=$(para_id "$out/$name-westend-local.json")
  paras+=("$id:$name")
done

"$bin/polkadot" build-spec --chain westend-local --disable-default-bootnode >"$out/westend-local-plain.json"
for para in "${paras[@]}"; do
  id=${para%%:*} name=${para#*:}
  # The genesis code is several MB, too large for a command-line argument; read it from the file.
  jq --argjson id "$id" --rawfile head "$out/$name.head" --rawfile code "$out/$name.wasm" \
    '.genesis.runtimeGenesis.patch.paras.paras += [[$id, {
      genesis_head: ($head | rtrimstr("\n")),
      validation_code: ($code | rtrimstr("\n")),
      parachain: true
    }]]' "$out/westend-local-plain.json" >"$out/westend-local-plain.json.new"
  mv "$out/westend-local-plain.json.new" "$out/westend-local-plain.json"
done
"$bin/polkadot" build-spec --chain "$out/westend-local-plain.json" --raw --disable-default-bootnode |
  jq --argjson boot "$(bootnodes "$relay_boot")" '.bootNodes = $boot' >"$out/westend-local.json"
rm -f "$out"/*.head "$out"/*.wasm "$out/westend-local-plain.json"
echo "generated $(jq -r '.id' "$out/westend-local.json") with paras ${paras[*]} in $out"

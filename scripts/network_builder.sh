#!/usr/bin/env bash
# shellcheck shell=bash
[ -z "${BASH_VERSION:-}" ] && exec /usr/bin/env bash "$0" "$@"

set -euo pipefail
IFS=$'\n\t'

# =====================
# Config & debug utils
# =====================
: "${DEBUG:=0}"
dbg() {
  if [ "$DEBUG" = "1" ]; then
    printf 'DEBUG: %s\n' "$*" >&2
  fi
}

# Prefer deterministic mktemp files under WORKDIR when possible
mktemp_wrk() {
  local pattern="$1"; shift || true
  # Fall back to default mktemp if WORKDIR is not yet created
  if [ -n "${WORKDIR:-}" ] && [ -d "$WORKDIR" ]; then
    mktemp "$WORKDIR/$pattern"
  else
    mktemp "$pattern"
  fi
}

# Portable canonical path (BSD/macOS & Linux)
canonical_path() {
  local p="$1"
  (cd "$(dirname -- "$p")" >/dev/null 2>&1 && printf '%s/%s\n' "$(pwd -P)" "$(basename -- "$p")")
}

# Hard fail if required command is missing
require_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "ERROR: '$1' not found" >&2; exit 1; }; }

# =====================
# Args
# =====================
if (( $# != 4 )); then
  echo "Usage: $0 <VALIDATORS 2..8> <PARACHAINS 0..8> <COLLATORS 0..8> <WORKDIR>"
  exit 1
fi

VALIDATORS="${1:-2}"
PARACHAINS="${2:-1}"
COLLATORS="${3:-1}"
WORKDIR="${4:-tmp}"

# Basic validation
rev='^[2-8]$'
re='^[0-8]$'
[[ "$VALIDATORS" =~ $rev ]] || { echo "VALIDATORS must be 2..8"; exit 1; }
[[ "$PARACHAINS" =~ $re ]] || { echo "PARACHAINS must be 0..8"; exit 1; }
[[ "$COLLATORS"  =~ $re ]] || { echo "COLLATORS must be 0..8";  exit 1; }

WORKDIR="$(canonical_path "$WORKDIR")"

if [ "${RMTMP:-0}" -eq 1 ] && [ -e "$WORKDIR" ]; then
  echo "Previous WORKDIR '$WORKDIR' was removed." >&2
  rm -Rf "$WORKDIR"
fi

if [ -e "$WORKDIR" ]; then
  echo "WORKDIR '$WORKDIR' already exists, remove it first." >&2
  exit 1
fi
mkdir -p -- "$WORKDIR"
dbg "WORKDIR $WORKDIR created"

# =====================
# Repos / constants
# =====================
POLKADOT_REPO="${POLKADOT_REPO:-$HOME/dev/polkadot-sdk}"
RELAYCHAIN="${RELAYCHAIN:-westend-local}"

PARA_BASE=${PARA_BASE:-2000}

# Separate base for relay p2p ports (collator's embedded relay node)
RELAY_P2P_BASE=${RELAY_P2P_BASE:-11000}

# --- host bandwidth (для всех хостов)
HOST_BW="${HOST_BW:-50 Mbit}"
HOST_BW_UP="${HOST_BW_UP:-${HOST_BW}}"
HOST_BW_DOWN="${HOST_BW_DOWN:-${HOST_BW}}"

# --- Shadow network defaults
# End-to-end link latency used for all edges in the Shadow graph
SHADOW_LATENCY="${SHADOW_LATENCY:-1 ms}"
# If > 0, the collator with this 1-based index in *each* parachain
# will be isolated from the other collators of the same parachain
# (packet_loss=1.0 for those edges only). Edges to validators stay intact.
ISOLATE_COLLATOR_IDX="${ISOLATE_COLLATOR_IDX:-0}"

# --- collatorSelection defaults (можно переопределить окружением)
COLLATOR_SELECTION_BOND="${COLLATOR_SELECTION_BOND:-0}"
# Если задать COLLATOR_SELECTION_DESIRED, будет использоваться оно,
# иначе desiredCandidates = числу invulnerables.
: "${COLLATOR_SELECTION_DESIRED:=}"

# --- glutton PoV sizing (runtime config)
# For pallet-glutton genesis, the keys expected in JSON are raw u64 integers representing FixedU64 inner values:
#   - storage: u64 representing FixedU64 (e.g., 1_000_000_000 = 1.0, 2_500_000_000 = 2.5)
#   - compute: u64 representing FixedU64 (e.g., 50_000_000 = 0.05, 500_000_000 = 0.5)
#   - blockLength: u64 representing FixedU64 (default 0 = no block body bloat)
#   - trashDataCount: u32 number of 1KiB entries pre-populated for PoV proofs (e.g. 5120 ~= 5MiB of keys).
# FixedU64 inner representation: 1.0 = 1_000_000_000 (1 billion), 0.5 = 500_000_000, etc.
# For maximum PoV: storage=2_500_000_000 (2.5 = 250%), compute=50_000_000 (0.05 = 5%)
GLUTTON_STORAGE="${GLUTTON_STORAGE:-2500000000}"
GLUTTON_COMPUTE="${GLUTTON_COMPUTE:-50000000}"
GLUTTON_BLOCK_LENGTH="${GLUTTON_BLOCK_LENGTH:-0}"
GLUTTON_TRASH_DATA_COUNT="${GLUTTON_TRASH_DATA_COUNT:-5120}"
# Validate formats: integers only
case "$GLUTTON_STORAGE" in (*[!0-9]*|"") echo "WARN: GLUTTON_STORAGE must be an integer; defaulting to 2'500'000'000" >&2; GLUTTON_STORAGE="2500000000";; esac
case "$GLUTTON_COMPUTE" in (*[!0-9]*|"") echo "WARN: GLUTTON_COMPUTE must be an integer; defaulting to 50'000'000" >&2; GLUTTON_COMPUTE="50000000";; esac
case "$GLUTTON_BLOCK_LENGTH" in (*[!0-9]*|"") echo "WARN: GLUTTON_BLOCK_LENGTH must be an integer; defaulting to 0" >&2; GLUTTON_BLOCK_LENGTH="0";; esac
case "$GLUTTON_TRASH_DATA_COUNT" in (*[!0-9]*|"") echo "WARN: GLUTTON_TRASH_DATA_COUNT must be an integer; defaulting to 5'120" >&2; GLUTTON_TRASH_DATA_COUNT=5120;; esac

LOGCFG="${LOGCFG:-info}"

# Ensure base toolchain
require_cmd cargo
require_cmd rustc
require_cmd jq
require_cmd awk
require_cmd xxd

# cd polkadot-sdk
# cargo build --profile testnet --features x-shadow,fast-runtime -p polkadot -p polkadot-parachain-bin
# cargo build --release -p staging-chain-spec-builder --bin chain-spec-builder
# cargo build --release -p glutton-westend-runtime
# cp -a target/release/wbuild/glutton-westend-runtime/glutton_westend_runtime.compact.compressed.wasm ./
# cargo install subxt-cli
# cargo install subkey

# =====================
# Locate/build required binaries
# (Use canonical_path instead of non-portable realpath)
# =====================
POLKADOT_BIN="$POLKADOT_REPO/target/testnet/polkadot"
if [[ -f "$POLKADOT_BIN" ]]; then
  POLKADOT_BIN="$(canonical_path "$POLKADOT_BIN")"
else
  echo "polkadot - not found; trying to build"
  cd "$POLKADOT_REPO"
  cargo build --profile testnet --features x-shadow -p polkadot
  POLKADOT_BIN="$(canonical_path "$POLKADOT_BIN")"
  [ -n "$POLKADOT_BIN" ] || { echo "polkadot - is not built"; exit 1; }
  cd -
fi
echo "polkadot - found: $POLKADOT_BIN"

COLLATOR_BIN="$POLKADOT_REPO/target/testnet/polkadot-parachain"
if [[ -f "$COLLATOR_BIN" ]]; then
  COLLATOR_BIN="$(canonical_path "$COLLATOR_BIN")"
else
  echo "polkadot-parachain - not found; trying to build"
  cd "$POLKADOT_REPO"
  cargo build --profile testnet --features x-shadow -p polkadot-parachain-bin
  COLLATOR_BIN="$(canonical_path "$COLLATOR_BIN")"
  [ -n "$COLLATOR_BIN" ] || { echo "polkadot-parachain - is not built"; exit 1; }
  cd -
fi
echo "polkadot-parachain - found: $COLLATOR_BIN"

SPEC_BUILDER_BIN="$POLKADOT_REPO/target/testnet/chain-spec-builder"
if [[ -f "$SPEC_BUILDER_BIN" ]]; then
  SPEC_BUILDER_BIN="$(canonical_path "$SPEC_BUILDER_BIN")"
else
  echo "chain-spec-builder - not found; trying to build"
  cd "$POLKADOT_REPO"
  cargo build --profile testnet -p staging-chain-spec-builder --bin chain-spec-builder
  SPEC_BUILDER_BIN="$(canonical_path "$SPEC_BUILDER_BIN")"
  [ -n "$SPEC_BUILDER_BIN" ] || { echo "chain-spec-builder - is not built"; exit 1; }
  cd -
fi
echo "chain-spec-builder - found: $SPEC_BUILDER_BIN"

if ! SUBKEY_BIN="$(command -v subkey 2>/dev/null)"; then
  echo "subkey - not found; trying to install"
  cargo install subkey
  SUBKEY_BIN="$(command -v subkey 2>/dev/null || true)"
  [ -n "$SUBKEY_BIN" ] || { echo "subkey - is not installed"; exit 1; }
fi
echo "subkey - found: $SUBKEY_BIN"

if ! SUBXT_BIN="$(command -v subxt 2>/dev/null)"; then
  echo "subxt - not found; trying to install"
  cargo install subxt-cli
  SUBXT_BIN="$(command -v subxt 2>/dev/null || true)"  # FIX: correct var
  [ -n "$SUBXT_BIN" ] || { echo "subxt - is not installed"; exit 1; }
fi
echo "subxt - found: $SUBXT_BIN"

RUNTIME_WASM="$POLKADOT_REPO/target/release/wbuild/glutton-westend-runtime/glutton_westend_runtime.compact.compressed.wasm"
if [[ -f "$RUNTIME_WASM" ]]; then
  RUNTIME_WASM="$(canonical_path "$RUNTIME_WASM")"
else
  echo "glutton_westend_runtime - not found; trying to build"
  cd "$POLKADOT_REPO"
  cargo build --release -p glutton-westend-runtime
  RUNTIME_WASM="$(canonical_path "target/release/wbuild/glutton-westend-runtime/glutton_westend_runtime.compact.compressed.wasm")"
  [ -f "$RUNTIME_WASM" ] || { echo "glutton_westend_runtime - is not built"; exit 1; }
  cd -
fi
echo "glutton_westend_runtime - found: $RUNTIME_WASM"

RELAY_SPEC_TMPL="relaychain-template.json"
if [[ -f "$RELAY_SPEC_TMPL" ]]; then
  RELAY_SPEC_TMPL="$(canonical_path "$RELAY_SPEC_TMPL")"
else
  echo "relaychain spec template - not found; trying to build"
  "$POLKADOT_BIN" build-spec \
    --chain "$RELAYCHAIN" \
    --disable-default-bootnode > relaychain-template.json 2>/dev/null
  RELAY_SPEC_TMPL="$(canonical_path relaychain-template.json)"
  [ -f "$RELAY_SPEC_TMPL" ] || { echo "relaychain spec template - is not built"; exit 1; }
fi
echo "relaychain spec template - found: $RELAY_SPEC_TMPL"

PARA_SPEC_TMPL="parachain-template.json"
if [[ -f "$PARA_SPEC_TMPL" ]]; then
  PARA_SPEC_TMPL="$(canonical_path "$PARA_SPEC_TMPL")"
else
  echo "parachain spec template - not found; trying to build"
  "$SPEC_BUILDER_BIN" create \
    -t local \
    --chain-name Parachain_1234 \
    --chain-id para1234 \
    --relay-chain "$RELAYCHAIN" \
    --para-id 1234 \
    --runtime "$RUNTIME_WASM" \
    named-preset local_testnet
  mv chain_spec.json parachain-template.json
  PARA_SPEC_TMPL="$(canonical_path parachain-template.json)"
  [ -f "$PARA_SPEC_TMPL" ] || { echo "parachain spec template - is not built"; exit 1; }
fi
echo "parachain spec template - found: $PARA_SPEC_TMPL"

lc() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

# =====================
# Cleanup intermediates
# =====================
clean() {
  dbg "clean"
  [ -n "${WORKDIR:-}" ] || { echo "ERROR: WORKDIR is not set" >&2; return 1; }
  [ -e "$WORKDIR" ] || { echo "Nothing to clean: $WORKDIR does not exist"; return 0; }

  # Keep list (implicit by not touching them):
  #   - $WORKDIR/nodes/**
  #   - *-raw.json
  #   - shadow.yaml

  # Explicit intermediate patterns to remove
  local patterns=(
    "$WORKDIR/paras.json"
    "$WORKDIR/tmp.*"
    "$WORKDIR/relaychain.json"
    "$WORKDIR/relaychain-no-code.json"
    "$WORKDIR/relaychain-val.json"
    "$WORKDIR/relaychain-val-no-code.json"
    "$WORKDIR/relaychain-val-paras.json"
    "$WORKDIR/relaychain-val-paras-no-code.json"
    "$WORKDIR/parachain.json"
    "$WORKDIR/parachain-no-code.json"
    "$WORKDIR/parachain-"[0-9]*"-no-code.json"
    "$WORKDIR/para-"[0-9]*"-genesis"
    "$WORKDIR/para-"[0-9]*"-wasm"
  )

  # Remove files matching patterns (without touching directories or keeps)
  local p f
  for p in "${patterns[@]}"; do
    # shellcheck disable=SC2086
    for f in $p; do
      [ -e "$f" ] || continue
      case "$f" in *-raw.json|*/shadow.yaml) continue ;; esac
      [ -d "$f" ] && continue
      dbg "rm -f -- $f"
      rm -f -- "$f" || { echo "ERROR: failed to remove $f" >&2; return 1; }
    done
  done
  dbg "Cleaned intermediates under $WORKDIR (kept nodes/, *-raw.json, shadow.yaml)"
}

# =====================
# Manifests & keys
# =====================
# prepare_manifest <Index> <Name>
# Prepares its manifest under $WORKDIR/nodes/<lower>/
# Does NOT create p2p secret; only writes the intended node-key file path.
# Manifest includes: controller & stash (sr25519) and full session_keys (babe, gran, imon, audi, para, asgn, beef),
# each with {suri, public_hex, secret_hex, ss58}. No top-level grandpa/beefy sections. (each session key also carries its ss58)
# Also includes: node_key_file, listen_address (without peer id suffix), rpc_port, prometheus_port.
prepare_manifest() {
  local index="${1:-}" name="${2:-}"
  [[ "$index" =~ ^[0-9]+$ ]] || { echo "prepare_manifest: index int required" >&2; return 1; }
  [ -n "$name" ] || { echo "prepare_manifest: name required" >&2; return 1; }
  [ -x "$SUBKEY_BIN" ] || { echo "ERROR: SUBKEY_BIN not executable" >&2; return 1; }

  local lower; lower="$(lc "$name")"
  local node_dir="$WORKDIR/nodes/$lower"
  mkdir -p "$node_dir"

  # Port bases and computed ports
  local P2P_BASE=10000 RPC_BASE=20000 PROM_BASE=30000
  local p2p_port=$((P2P_BASE + index))
  local p2p_port_relay=$((RELAY_P2P_BASE + index))
  local rpc_port=$((RPC_BASE + index))
  local prom_port=$((PROM_BASE + index))
  dbg "ports: p2p=$p2p_port relay_p2p=$p2p_port_relay rpc=$rpc_port prom=$prom_port"

  # Node key as hex (ed25519 secret) and listen address (no peer id)
  # We use the ed25519 secret hex as the libp2p node key material for dev.
  local ip_octet=$((index+1))
  local listen_addr
  local listen_addr_relay
  if [ -n "${USE_LOCALHOST:-}" ]; then
    listen_addr="/ip4/127.0.0.1/tcp/${p2p_port}"
    listen_addr_relay="/ip4/127.0.0.1/tcp/${p2p_port_relay}"
  else
    local ip_prefix="10.0.0";
    listen_addr="/ip4/${ip_prefix}.${ip_octet}/tcp/${p2p_port}"
    listen_addr_relay="/ip4/${ip_prefix}.${ip_octet}/tcp/${p2p_port_relay}"
  fi

  # Helper to get keys
  get_key() {
    local scheme="$1" suri="$2" out ss58
    out="$("$SUBKEY_BIN" inspect --scheme "$scheme" "$suri" 2>&1)" || return 1
    if [ "$scheme" = ecdsa ]; then
      ss58="$(printf '%s\n' "$out" | awk -F': ' 'BEGIN{IGNORECASE=1} /Public key \(SS58\)/{gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}')"
    else
      ss58="$(printf '%s\n' "$out" | awk -F': ' 'BEGIN{IGNORECASE=1} /SS58[[:space:]]+Address/{gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit}')"
    fi
    [ -n "$ss58" ] || { echo "ERROR: SS58 parse failed for $scheme $suri" >&2; return 1; }
    printf '%s' "$ss58"
  }

  get_hex_pair() {
    local scheme="$1" suri="$2" out pub sec
    out="$("$SUBKEY_BIN" inspect --scheme "$scheme" "$suri" 2>&1)" || return 1
    pub="$(printf '%s\n' "$out" | awk -F': ' 'BEGIN{IGNORECASE=1}/Public key \(hex\)/{gsub(/^[ \\t]+|[ \\t]+$/, "", $2); print $2; exit}')"
    sec="$(printf '%s\n' "$out" | awk -F': ' 'BEGIN{IGNORECASE=1}/Secret key \(hex\)/{gsub(/^[ \\t]+|[ \\t]+$/, "", $2); print $2; exit}')"
    [ -n "$sec" ] || sec="$(printf '%s\n' "$out" | awk -F': ' 'BEGIN{IGNORECASE=1}/Secret seed/{gsub(/^[ \\t]+|[ \\t]+$/, "", $2); print $2; exit}')"
    [ -n "$pub" ] && [ -n "$sec" ] || { echo "ERROR: hex parse failed for $scheme $suri" >&2; return 1; }
    printf '%s %s' "$pub" "$sec"
  }

  local controller_ss58 grandpa_ss58 beefy_ss58 stash_ss58
  controller_ss58="$(get_key sr25519 "//$name")" || return 1
  grandpa_ss58="$(get_key ed25519 "//$name")" || return 1
  beefy_ss58="$(get_key ecdsa   "//$name")" || return 1
  stash_ss58="$(get_key sr25519 "//$name//stash")" || return 1
  dbg "ss58: controller=$controller_ss58 grandpa=$grandpa_ss58 beefy=$beefy_ss58 stash=$stash_ss58"

  local sr_pub sr_sec ed_pub ed_sec ec_pub ec_sec stash_pub stash_sec tmp_pair
  tmp_pair="$(get_hex_pair sr25519 "//$name")"; sr_pub="${tmp_pair%% *}"; sr_sec="${tmp_pair#* }"
  tmp_pair="$(get_hex_pair ed25519 "//$name")"; ed_pub="${tmp_pair%% *}"; ed_sec="${tmp_pair#* }"
  tmp_pair="$(get_hex_pair ecdsa "//$name")";    ec_pub="${tmp_pair%% *}"; ec_sec="${tmp_pair#* }"
  tmp_pair="$(get_hex_pair sr25519 "//$name//stash")"; stash_pub="${tmp_pair%% *}"; stash_sec="${tmp_pair#* }"
  dbg "hex: sr_pub=$sr_pub ed_pub=$ed_pub ec_pub=$ec_pub stash_pub=$stash_pub"

  # Separate ed25519 key for relay side (so relay libp2p PeerId differs)
  local rel_ed_pub rel_ed_sec
  tmp_pair="$(get_hex_pair ed25519 "//$name//relay")"; rel_ed_pub="${tmp_pair%% *}"; rel_ed_sec="${tmp_pair#* }"

  # Derive PeerId from the ed25519 secret (hex) via stdin to polkadot (no fallbacks)
  local peer_id=""
  if command -v "$POLKADOT_BIN" >/dev/null 2>&1; then
    dbg "peerid: via polkadot key inspect-node-key (stdin)"
    peer_id="$("$POLKADOT_BIN" key inspect-node-key <<<"${ed_sec}" 2>/dev/null | tr -d '\r\n' | head -c 200)" || peer_id=""
    dbg "peerid: result='${peer_id:-}'"
  fi

  local relay_peer_id=""
  if command -v "$POLKADOT_BIN" >/dev/null 2>&1; then
    relay_peer_id="$("$POLKADOT_BIN" key inspect-node-key <<<"${rel_ed_sec}" 2>/dev/null | tr -d '\r\n' | head -c 200)" || relay_peer_id=""
  fi

  # Build session keys array with suri, public_hex, secret_hex, ss58
  local session_json
  session_json="$(jq -cn \
    --arg suri_sr "//$name" --arg suri_ed "//$name" --arg suri_ec "//$name" \
    --arg pub_sr "$sr_pub"  --arg sec_sr "$sr_sec" --arg ss58_sr "$controller_ss58" \
    --arg pub_ed "$ed_pub"  --arg sec_ed "$ed_sec" --arg ss58_ed "$grandpa_ss58" \
    --arg pub_ec "$ec_pub"  --arg sec_ec "$ec_sec" --arg ss58_ec "$beefy_ss58" '
    [
      {type:"babe", scheme:"sr25519", suri:$suri_sr, public_hex:$pub_sr, secret_hex:$sec_sr, ss58:$ss58_sr},
      {type:"aura", scheme:"sr25519", suri:$suri_sr, public_hex:$pub_sr, secret_hex:$sec_sr, ss58:$ss58_sr},
      {type:"gran", scheme:"ed25519", suri:$suri_ed, public_hex:$pub_ed, secret_hex:$sec_ed, ss58:$ss58_ed},
      {type:"imon", scheme:"sr25519", suri:$suri_sr, public_hex:$pub_sr, secret_hex:$sec_sr, ss58:$ss58_sr},
      {type:"audi", scheme:"sr25519", suri:$suri_sr, public_hex:$pub_sr, secret_hex:$sec_sr, ss58:$ss58_sr},
      {type:"para", scheme:"sr25519", suri:$suri_sr, public_hex:$pub_sr, secret_hex:$sec_sr, ss58:$ss58_sr},
      {type:"asgn", scheme:"sr25519", suri:$suri_sr, public_hex:$pub_sr, secret_hex:$sec_sr, ss58:$ss58_sr},
      {type:"beef", scheme:"ecdsa",   suri:$suri_ec, public_hex:$pub_ec, secret_hex:$sec_ec, ss58:$ss58_ec}
    ]')"

  dbg "writing manifest to $node_dir/manifest.json"
  jq -n --arg name "$name" \
    --arg controller_suri "//$name" \
    --arg controller_ss58 "$controller_ss58" \
    --arg controller_pub "$sr_pub" \
    --arg controller_sec "$sr_sec" \
    --arg stash_suri "//$name//stash" \
    --arg stash_ss58 "$stash_ss58" \
    --arg stash_pub "$stash_pub" \
    --arg stash_sec "$stash_sec" \
    --arg node_key_para "$ed_sec" \
    --arg listen_addr_para "$listen_addr" \
    --arg peer_id_para "$peer_id" \
    --arg node_key_relay "$rel_ed_sec" \
    --arg listen_addr_relay "$listen_addr_relay" \
    --arg peer_id_relay "$relay_peer_id" \
    --argjson rpc_port "$rpc_port" \
    --argjson prometheus_port "$prom_port" \
    --argjson session_keys "$session_json" '
    {
      name: $name,
      controller: { scheme: "sr25519", suri: $controller_suri, ss58: $controller_ss58, public_hex: $controller_pub, secret_hex: $controller_sec },
      stash:      { scheme: "sr25519", suri: $stash_suri,      ss58: $stash_ss58,      public_hex: $stash_pub,      secret_hex: $stash_sec },
      session_keys: $session_keys,
      # Backward-compatible flat fields (for parachain side by default)
      node_key: $node_key_para,
      listen_address: $listen_addr_para,
      peer_id: $peer_id_para,
      # Structured network section
      network: {
        para:  { listen_address: $listen_addr_para,  node_key: $node_key_para,  peer_id: $peer_id_para },
        relay: { listen_address: $listen_addr_relay, node_key: $node_key_relay, peer_id: $peer_id_relay }
      },
      rpc_port: $rpc_port,
      prometheus_port: $prometheus_port
    }' > "$node_dir/manifest.json"

  # For validators, flat keys should refer to relay side (no parachain process)
  if [[ "$name" == Validator_* ]]; then
    jq --arg la "$listen_addr_relay" --arg nk "$rel_ed_sec" --arg pid "$relay_peer_id" '
      .listen_address = $la | .node_key = $nk | .peer_id = $pid
    ' "$node_dir/manifest.json" > "$node_dir/manifest.json.tmp" && mv "$node_dir/manifest.json.tmp" "$node_dir/manifest.json"
  fi
  echo "Prepared manifest for $name (index $index): $node_dir/manifest.json"
}

clean_dev_validators_patch() {
  local spec_path="$1"
  if [ -z "$spec_path" ] || [ ! -f "$spec_path" ]; then
    echo "clean_dev_validators_patch: Spec file not found: $spec_path" >&2
    return 1
  fi
  local tmp_out
  tmp_out="$(mktemp "$WORKDIR/tmp.clean.XXXXXX")"
  jq '
    .genesis = (.genesis // {}) |
    .genesis.runtimeGenesis = (.genesis.runtimeGenesis // {}) |
    .genesis.runtimeGenesis.patch = (.genesis.runtimeGenesis.patch // {}) |
    .genesis.runtimeGenesis.patch.session = (.genesis.runtimeGenesis.patch.session // {}) |
    .genesis.runtimeGenesis.patch.session.keys = [] |
    .genesis.runtimeGenesis.patch.balances = (.genesis.runtimeGenesis.patch.balances // {}) |
    .genesis.runtimeGenesis.patch.balances.balances = [] |
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch | has("staking")))
      then
        .genesis.runtimeGenesis.patch.staking = (
          .genesis.runtimeGenesis.patch.staking
          | .forceEra = "NotForcing"
          | .invulnerables = []
          | .minimumValidatorCount = 1
          | .slashRewardFraction = 100000000
          | .stakers = []
          | .validatorCount = 0
        )
      else . end
    ) |
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.aura? // null) != null
          and (.genesis.runtimeGenesis.config.aura | has("authorities")))
      then
        .genesis.runtimeGenesis.config.aura.authorities = []
      else . end
    ) |
    .bootNodes = []
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
  echo "Validator session keys, balances, and bootNodes cleared in $spec_path"
}

# clean_dev_collators_patch <SPEC_JSON> <PARA_ID>
# Clears balances and session keys and sets parachain ids to PARA_ID
clean_dev_collators_patch() {
  local spec_path="$1" para_id="$2"
  if [ -z "$spec_path" ] || [ ! -f "$spec_path" ]; then
    echo "clean_dev_collators_patch: Spec file not found: $spec_path" >&2; return 1; fi
  if ! printf '%s' "$para_id" | grep -Eq '^[0-9]+$'; then
    echo "clean_dev_collators_patch: PARA_ID must be an integer" >&2; return 1; fi

  local tmp_out
  tmp_out="$(mktemp "$WORKDIR/tmp.cleanpara.XXXXXX")"
  jq --argjson pid "$para_id" --arg bond_str "$COLLATOR_SELECTION_BOND" '
    .id = ("para" + ($pid|tostring)) |
    .name = ("Parachain_" + ($pid|tostring)) |
    .para_id = $pid |
    .genesis = (.genesis // {}) |
    .genesis.runtimeGenesis = (.genesis.runtimeGenesis // {}) |
    .genesis.runtimeGenesis.patch = (.genesis.runtimeGenesis.patch // {}) |

    # patch.parachainInfo.parachainId := pid (если есть)
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.parachainInfo? // null) != null
          and (.genesis.runtimeGenesis.patch.parachainInfo | has("parachainId")))
      then
        .genesis.runtimeGenesis.patch.parachainInfo.parachainId = $pid
      else . end
    ) |

    # patch.sudo.key := null (если есть)
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.sudo? // null) != null
          and (.genesis.runtimeGenesis.patch.sudo | has("key")))
      then
        .genesis.runtimeGenesis.patch.sudo.key = null
      else . end
    ) |

    # config.parachainInfo.parachainId := pid (если есть)
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.parachainInfo? // null) != null
          and (.genesis.runtimeGenesis.config.parachainInfo | has("parachainId")))
      then
        .genesis.runtimeGenesis.config.parachainInfo.parachainId = $pid
      else . end
    ) |

    # ==== collatorSelection (CONFIG): создать и очистить ====
    .genesis.runtimeGenesis.config = (.genesis.runtimeGenesis.config // {}) |
    .genesis.runtimeGenesis.config.collatorSelection = {
      invulnerables: [],
      candidacyBond: $bond_str,
      desiredCandidates: 0
    } |

    # ==== AURA: очистка и в PATCH, и в CONFIG (если присутствуют) ====
    # patch.aura.authorities := []
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.aura? // null) != null
          and (.genesis.runtimeGenesis.patch.aura | has("authorities")))
      then
        .genesis.runtimeGenesis.patch.aura.authorities = []
      else . end
    ) |
    # config.aura.authorities := []
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.aura? // null) != null
          and (.genesis.runtimeGenesis.config.aura | has("authorities")))
      then
        .genesis.runtimeGenesis.config.aura.authorities = []
      else . end
    ) |

    # дублирующая защита: если config.collatorSelection уже есть, зануляем invulnerables
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.collatorSelection? // null) != null
          and (.genesis.runtimeGenesis.config.collatorSelection | has("invulnerables")))
      then
        .genesis.runtimeGenesis.config.collatorSelection.invulnerables = []
      else . end
    ) |

    # balances (CONFIG → [], PATCH → [])
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.balances? // null) != null
          and (.genesis.runtimeGenesis.config.balances | has("balances")))
      then
        .genesis.runtimeGenesis.config.balances.balances = []
      else . end
    ) |
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.balances? // null) != null
          and (.genesis.runtimeGenesis.patch.balances | has("balances")))
      then
        .genesis.runtimeGenesis.patch.balances.balances = []
      else . end
    ) |

    # session.keys (PATCH → [])
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.session? // null) != null
          and (.genesis.runtimeGenesis.patch.session | has("keys")))
      then
        .genesis.runtimeGenesis.patch.session.keys = []
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
  echo "Parachain patch cleaned and para_id set to $para_id in $spec_path"
}

#
# add_dev_validators_patch <SPEC_JSON> <VALIDATOR_NAME>
# Adds or updates a validator's session keys, balances, and bootNodes in the patch section.
add_dev_validators_patch() {
  local spec_path="$1"
  local validator_name="$2"
  if [ -z "$spec_path" ] || [ ! -f "$spec_path" ]; then
      echo "add_dev_validators_patch: Spec file not found: $spec_path" >&2
      return 1
  fi
  if [ -z "$validator_name" ]; then
      echo "add_dev_validators_patch: Validator name required" >&2
      return 1
  fi
  local lower
  lower="$(lc "$validator_name")"
  local manifest="$WORKDIR/nodes/$lower/manifest.json"
  if [ ! -f "$manifest" ]; then
      echo "add_dev_validators_patch: Manifest not found for validator $validator_name at $manifest" >&2
      return 1
  fi
    # Read controller, grandpa, beefy addresses
  local controller_ss58 grandpa_ss58 beefy_ss58 stash_ss58
  controller_ss58="$(jq -r '.controller.ss58' "$manifest")"
  stash_ss58="$(jq -r '.stash.ss58' "$manifest")"
  grandpa_ss58="$(jq -r '.session_keys[]|select(.type=="gran")|.ss58' "$manifest")"
  beefy_ss58="$(jq -r '.session_keys[]|select(.type=="beef")|.ss58' "$manifest")"
  if [ -z "$controller_ss58" ] || [ -z "$grandpa_ss58" ] || [ -z "$beefy_ss58" ] || [ -z "$stash_ss58" ]; then
      echo "add_dev_validators_patch: Missing key data in manifest for $validator_name" >&2
      return 1
  fi
    # Check if this is the first validator being added (before modifying session keys)
  local is_first
  is_first="$(jq -r '((.genesis.runtimeGenesis.patch.session.keys // []) | length) == 0' "$spec_path")"
    # Build session key entry
  local session_entry
  session_entry="$(jq -cn \
      --arg controller "$controller_ss58" \
      --arg grandpa "$grandpa_ss58" \
      --arg beefy "$beefy_ss58" \
      '[ $controller, $controller, {
          authority_discovery: $controller,
          babe: $controller,
          beefy: $beefy,
          grandpa: $grandpa,
          para_assignment: $controller,
          para_validator: $controller
      } ]')"
    # Remove any existing entry with same controller, append new one
  local tmp_out
  tmp_out="$(mktemp "$WORKDIR/tmp.addval.XXXXXX")"
  jq --argjson new_entry "$session_entry" --arg controller "$controller_ss58" '
      .genesis = (.genesis // {}) |
      .genesis.runtimeGenesis = (.genesis.runtimeGenesis // {}) |
      .genesis.runtimeGenesis.patch = (.genesis.runtimeGenesis.patch // {}) |
      .genesis.runtimeGenesis.patch.session = (.genesis.runtimeGenesis.patch.session // {}) |
      .genesis.runtimeGenesis.patch.session.keys = (
        (.genesis.runtimeGenesis.patch.session.keys // [])
        | map(select(.[0] != $controller))
        + [$new_entry]
      )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
    # If this is the first added validator, set Sudo key to its controller address
  if [ "$is_first" = "true" ]; then
    tmp_out="$(mktemp "$WORKDIR/tmp.addval.XXXXXX")"
    jq --arg controller "$controller_ss58" '
      .genesis = (.genesis // {}) |
      .genesis.runtimeGenesis = (.genesis.runtimeGenesis // {}) |
      .genesis.runtimeGenesis.patch = (.genesis.runtimeGenesis.patch // {}) |
      .genesis.runtimeGenesis.patch.sudo = (
        (.genesis.runtimeGenesis.patch.sudo // {})
      ) |
      .genesis.runtimeGenesis.patch.sudo.key = $controller
    ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
    echo "Sudo key set to controller of $validator_name"
  fi
    # Add/update balances for controller and stash
  local amount_controller_default="1000000000000000000"
  local amount_stash_default="1000000000000000000"
  tmp_out="$(mktemp "$WORKDIR/tmp.addval.XXXXXX")"
  jq --arg controller "$controller_ss58" --arg stash "$stash_ss58" \
     --argjson amt_controller "$amount_controller_default" \
     --argjson amt_stash "$amount_stash_default" '
    .genesis = (.genesis // {}) |
    .genesis.runtimeGenesis = (.genesis.runtimeGenesis // {}) |
    .genesis.runtimeGenesis.patch = (.genesis.runtimeGenesis.patch // {}) |
    .genesis.runtimeGenesis.patch.balances = (.genesis.runtimeGenesis.patch.balances // {}) |
    .genesis.runtimeGenesis.patch.balances.balances = (
      (.genesis.runtimeGenesis.patch.balances.balances // [])
      | map(select(.[0] != $controller and .[0] != $stash))
      + [[$controller, $amt_controller], [$stash, $amt_stash]]
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

    # Add/update staking for the validator (controller acts as both stash and controller)
  local amount_bonded_default="100000000000000"
  tmp_out="$(mktemp "$WORKDIR/tmp.addval.XXXXXX")"
  jq --arg controller "$controller_ss58" \
     --argjson amt_bonded "$amount_bonded_default" '
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch | has("staking")))
      then
        .genesis.runtimeGenesis.patch.staking = (
          .genesis.runtimeGenesis.patch.staking
          | .forceEra = "NotForcing"
          | .minimumValidatorCount = (.minimumValidatorCount // 1)
          | .slashRewardFraction = (.slashRewardFraction // 100000000)
          | .invulnerables = (((.invulnerables // []) + [$controller]) | unique)
          | .stakers = ((.stakers // [])
              | map(select(.[0] != $controller))
              + [[ $controller, $controller, $amt_bonded, "Validator" ]])
          | .validatorCount = ((.invulnerables // []) | length)
        )
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

    # Append bootNode
  local listen_address peer_id bootnode
  listen_address="$(jq -r '.listen_address' "$manifest")"
  peer_id="$(jq -r '.peer_id' "$manifest")"
  if [ -n "$listen_address" ] && [ -n "$peer_id" ]; then
      bootnode="${listen_address}/p2p/${peer_id}"
      tmp_out="$(mktemp "$WORKDIR/tmp.addval.XXXXXX")"
      jq --arg bootnode "$bootnode" '
        .bootNodes = ((.bootNodes // []) + [$bootnode] | unique)
      ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
  fi
  echo "Validator $validator_name added or updated in $spec_path"
}

# add_dev_collators_patch <SPEC_JSON> <COLLATOR_NAME>
# Adds Aura session key entry for a collator from its manifest
add_dev_collators_patch() {
  local spec_path="$1" collator_name="$2"
  if [ -z "$spec_path" ] || [ ! -f "$spec_path" ]; then
    echo "add_dev_collators_patch: Spec file not found: $spec_path" >&2; return 1; fi
  if [ -z "$collator_name" ]; then
    echo "add_dev_collators_patch: Collator name required" >&2; return 1; fi
  local lower manifest controller_ss58 aura_ss58 tmp_out
  lower="$(lc "$collator_name")"
  manifest="$WORKDIR/nodes/$lower/manifest.json"
  if [ ! -f "$manifest" ]; then
    echo "add_dev_collators_patch: Manifest not found for $collator_name at $manifest" >&2; return 1; fi
  controller_ss58="$(jq -r '.controller.ss58' "$manifest")"
  aura_ss58="$(jq -r '.session_keys[]|select(.type=="aura")|.ss58' "$manifest")"
  if [ -z "$controller_ss58" ] || [ -z "$aura_ss58" ]; then
    echo "add_dev_collators_patch: Missing controller/aura ss58 in manifest for $collator_name" >&2; return 1; fi
  # Check if this is the first collator being added
  local is_first
  is_first="$(jq -r '((.genesis.runtimeGenesis.patch.session.keys // []) | length) == 0' "$spec_path")"
  # Build session key entry with only Aura for parachain
  local entry
  entry="$(jq -cn --arg acc "$controller_ss58" --arg aura "$aura_ss58" '[ $acc, $acc, { aura: $aura } ]')"
  tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
  jq --argjson new_entry "$entry" --arg acc "$controller_ss58" '
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.session? // null) != null
          and (.genesis.runtimeGenesis.patch.session | has("keys")))
      then
        .genesis.runtimeGenesis.patch.session.keys = (
          (.genesis.runtimeGenesis.patch.session.keys // [])
          | map(select(.[0] != $acc))
          + [$new_entry]
        )
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

  # If patch.aura.authorities exists, add the collator aura there (unique)
  tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
  jq --arg aura "$aura_ss58" '
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.aura? // null) != null
          and (.genesis.runtimeGenesis.patch.aura | has("authorities")))
      then
        .genesis.runtimeGenesis.patch.aura.authorities = (
          ((.genesis.runtimeGenesis.patch.aura.authorities // [])
            | map(select(. != $aura)))
          + [$aura]
        )
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

  # Mirror authorities → invulnerables (CONFIG only; preserve order)
  tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
  jq '
    .genesis //= {} |
    .genesis.runtimeGenesis //= {} |
    .genesis.runtimeGenesis.config //= {} |
    .genesis.runtimeGenesis.config.collatorSelection
      = (.genesis.runtimeGenesis.config.collatorSelection // {}) |
    (
      if (.genesis.runtimeGenesis.patch? // {} | .aura? // {} | .authorities? // null) != null
      then
        .genesis.runtimeGenesis.config.collatorSelection.invulnerables
          = (.genesis.runtimeGenesis.patch.aura.authorities)
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

  # If this is the first collator being added, set Sudo key to its controller address, and mirror to config.sudo.key if it exists
  if [ "$is_first" = "true" ]; then
    tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
    jq --arg controller "$controller_ss58" '
      (
        if ((.genesis? // null) != null
            and (.genesis.runtimeGenesis? // null) != null
            and (.genesis.runtimeGenesis.patch? // null) != null
            and (.genesis.runtimeGenesis.patch.sudo? // null) != null
            and (.genesis.runtimeGenesis.patch.sudo | has("key")))
        then
          .genesis.runtimeGenesis.patch.sudo.key = $controller
        else . end
      ) |
      (
        if ((.genesis? // null) != null
            and (.genesis.runtimeGenesis? // null) != null
            and (.genesis.runtimeGenesis.config? // null) != null
            and (.genesis.runtimeGenesis.config.sudo? // null) != null
            and (.genesis.runtimeGenesis.config.sudo | has("key")))
        then
          .genesis.runtimeGenesis.config.sudo.key = $controller
        else . end
      )
    ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
    echo "Parachain Sudo key set to controller of $collator_name"
  fi

  # --- ensure CONFIG.collatorSelection exists, set bond & desired (do not touch invulnerables)
  tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
  jq --arg bond_str "$COLLATOR_SELECTION_BOND" '
    .genesis //= {} |
    .genesis.runtimeGenesis //= {} |
    .genesis.runtimeGenesis.config //= {} |
    .genesis.runtimeGenesis.config.collatorSelection
      = (.genesis.runtimeGenesis.config.collatorSelection // {}) |
    .genesis.runtimeGenesis.config.collatorSelection
      |= (
        .candidacyBond = $bond_str
        | .desiredCandidates =
            ( ( (env.COLLATOR_SELECTION_DESIRED // "") | tonumber? )
              // ((.invulnerables // []) | length) )
      )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

  # Fund collator controller in balances (patch + config sync)
  local amount_collator_default="1000000000000000000"
  tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
  jq --arg controller "$controller_ss58" \
     --argjson amt "$amount_collator_default" '
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.patch? // null) != null
          and (.genesis.runtimeGenesis.patch.balances? // null) != null
          and (.genesis.runtimeGenesis.patch.balances | has("balances")))
      then
        .genesis.runtimeGenesis.patch.balances.balances = (
          (.genesis.runtimeGenesis.patch.balances.balances // [])
          | map(select(.[0] != $controller))
          + [[ $controller, $amt ]]
        )
      else . end
    ) |
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.balances? // null) != null
          and (.genesis.runtimeGenesis.config.balances | has("balances")))
      then
        .genesis.runtimeGenesis.config.balances.balances = (
          ((.genesis.runtimeGenesis.config.balances.balances // [])
            | map(select(.[0] != $controller)))
          + [[ $controller, $amt ]]
        )
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"

  # Sync config.aura.authorities if present (do not create branches)
  tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
  jq --arg aura "$aura_ss58" '
    (
      if ((.genesis? // null) != null
          and (.genesis.runtimeGenesis? // null) != null
          and (.genesis.runtimeGenesis.config? // null) != null
          and (.genesis.runtimeGenesis.config.aura? // null) != null
          and (.genesis.runtimeGenesis.config.aura | has("authorities")))
      then
        .genesis.runtimeGenesis.config.aura.authorities = (
          ((.genesis.runtimeGenesis.config.aura.authorities // [])
            | map(select(. != $aura)))
          + [$aura]
        )
      else . end
    )
  ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"


  # Append bootNode for collator
  local listen_address peer_id bootnode
  listen_address="$(jq -r '.listen_address // empty' "$manifest")"
  peer_id="$(jq -r '.peer_id // empty' "$manifest")"
  if [ -n "$listen_address" ] && [ -n "$peer_id" ]; then
    bootnode="${listen_address}/p2p/${peer_id}"
    tmp_out="$(mktemp "$WORKDIR/tmp.addcol.XXXXXX")"
    jq --arg bootnode "$bootnode" '
      .bootNodes = ((.bootNodes // []) + [$bootnode] | unique)
    ' "$spec_path" > "$tmp_out" && mv "$tmp_out" "$spec_path"
  fi

  echo "Collator $collator_name (Aura) added to $spec_path"
}


# replace_runtime_code <INPUT_SPEC.json> <OUTPUT_SPEC.json> [HEX_CODE]
# Writes HEX_CODE (default: 0xdeadcode) into .genesis.runtimeGenesis.code, .genesis.raw.top["0x3a636f6465"], and .genesis.runtimeGenesis.patch.paras.paras[*][1][1], only if those keys exist.
replace_runtime_code() {
  local input_spec_path="$1" output_spec_path="$2" new_code="${3:-0xdeadcode}"
  [ -f "$input_spec_path" ] || { echo "input spec not found: $input_spec_path" >&2; return 1; }
  [ -n "$output_spec_path" ] || { echo "output path required" >&2; return 1; }
  jq --arg code "$new_code" '
    (if (.genesis? // null) != null and (.genesis.runtimeGenesis? // null) != null and (.genesis.runtimeGenesis | has("code")) then .genesis.runtimeGenesis.code = $code else . end) |
    (if (.genesis? // null) != null and (.genesis.raw? // null) != null and (.genesis.raw.top? // null) != null and (.genesis.raw.top | has("0x3a636f6465")) then .genesis.raw.top["0x3a636f6465"] = $code else . end) |
    (if (.genesis? // null) != null and (.genesis.runtimeGenesis? // null) != null and (.genesis.runtimeGenesis.patch? // null) != null and (.genesis.runtimeGenesis.patch.paras? // null) != null and (.genesis.runtimeGenesis.patch.paras.paras? // null) != null then .genesis.runtimeGenesis.patch.paras.paras = (.genesis.runtimeGenesis.patch.paras.paras | map(if (type == "array" and length >= 2 and (.[1] | type) == "array" and (.[1] | length) >= 2) then (.[1][1] = $code) else . end)) else . end)
  ' "$input_spec_path" > "$output_spec_path"
  dbg "replace_runtime_code: output: $output_spec_path"
}


# provision_node_keys <Name> <SPEC_JSON>
provision_node_keys() {
  local validator_name="$1"
  local spec_json="$2"

  # --- validate tools & args ---
  if [ -z "${POLKADOT_BIN:-}" ] || [ ! -x "$POLKADOT_BIN" ]; then
    echo "ERROR: POLKADOT_BIN is not set/executable" >&2; return 1; fi
  if [ -z "${SUBKEY_BIN:-}" ] || [ ! -x "$SUBKEY_BIN" ]; then
    echo "ERROR: SUBKEY_BIN is not set/executable" >&2; return 1; fi
  if [ -z "$validator_name" ] || [ -z "$spec_json" ] || [ ! -f "$spec_json" ]; then
    echo "Usage: provision_node_keys <Name> <SPEC_JSON>" >&2; return 1; fi

  local lower node_dir base_path
  lower="$(echo "$validator_name" | tr '[:upper:]' '[:lower:]')"
  node_dir="$WORKDIR/nodes/$lower"
  base_path="$node_dir/base"
  mkdir -p "$base_path" || { echo "ERROR: cannot mkdir -p $base_path" >&2; return 1; }

  # Validate manifest exists
  if [ ! -f "$node_dir/manifest.json" ]; then
    echo "ERROR: manifest not found for validator $validator_name at $node_dir/manifest.json" >&2
    return 1
  fi

  # --- chain id ---
  local chain_id
  chain_id="$(jq -r '.id // empty' "$spec_json")"
  if [ -z "$chain_id" ]; then echo "ERROR: .id not found in $spec_json" >&2; return 1; fi

  dbg "provision_node_keys: $base_path (chain: $chain_id) for $validator_name (from manifest) =="

  # --- session keys from manifest ---
  # Mapping: babe->babe, imon->imon, audi->audi, para->para, asgn->asgn, gran->gran, beef->beef
  local ktype kscheme suri
  local insert_fail=0
  while read -r key; do
    ktype="$(echo "$key" | jq -r '.type')"
    kscheme="$(echo "$key" | jq -r '.scheme')"
    suri="$(echo "$key" | jq -r '.suri')"
    [ -z "$ktype" ] && continue
    if "$POLKADOT_BIN" key insert \
      --base-path "$base_path" \
      --chain "$spec_json" \
      --key-type "$ktype" \
      --scheme "$kscheme" \
      --suri "$suri" >/dev/null 2>&1; then
      dbg "  + inserted $ktype ($kscheme) from manifest"
    else
      echo "ERROR: failed to insert $ktype ($kscheme) from manifest for $validator_name" >&2
      insert_fail=1
      break
    fi
  done < <(jq -c '.session_keys[]' "$node_dir/manifest.json")

  if [ "$insert_fail" -ne 0 ]; then
    return 1
  fi
  dbg "Session keystore populated from manifest: $base_path (chain: $chain_id) for $validator_name"

  # --- p2p key from manifest ---
  local p2p_dir="$base_path/chains/$chain_id/network"
  local p2p_file="$p2p_dir/secret_ed25519"
  mkdir -p "$p2p_dir" || { echo "ERROR: cannot mkdir -p $p2p_dir" >&2; return 1; }
  local node_key_hex
  node_key_hex="$(jq -r '.node_key // empty' "$node_dir/manifest.json")"
  dbg "node_key_hex set (len: ${#node_key_hex})"
  if [ -z "$node_key_hex" ]; then echo "ERROR: node_key not found in manifest" >&2; return 1; fi
  printf '%s' "$node_key_hex" | xxd -r -p > "$p2p_file" || { echo "ERROR: failed to write p2p secret from manifest" >&2; return 1; }
  chmod 600 "$p2p_file"
  dbg "P2P secret written from manifest: $p2p_file"

  # --- read PeerId for log (prefer polkadot key inspect-node-key --file) ---
  local peer_id=""
#  if "$POLKADOT_BIN" key inspect-node-key --help 2>&1 | grep -q -- '--file'; then
#    peer_id="$(cat "$p2p_file" | "$POLKADOT_BIN" key inspect-node-key --bin 2>/dev/null \
#      | awk -F': ' 'BEGIN{IGNORECASE=1}/Peer[[:space:]]*ID/{print $2; exit}')"
#  fi
  if "$POLKADOT_BIN" key inspect-node-key --help 2>&1 | grep -q -- '--bin'; then
    peer_id="$(cat "$p2p_file" | "$POLKADOT_BIN" key inspect-node-key --bin 2>/dev/null | tr -d '\r\n')"
  fi
  if [ -z "$peer_id" ]; then
    # Last resort: match 12D3Koo…-like
    peer_id="$(strings "$p2p_file" 2>/dev/null | grep -Eo '12D3Koo[1-9A-HJ-NP-Za-km-z]+' | head -n1)"
  fi
  [ -n "$peer_id" ] && echo "PeerId (from manifest p2p key): $peer_id"

  echo "$validator_name provisioned by keys from manifest"
}

patch_relay_with_paras() {
  local relay_in="$1" paras_json="$2" relay_out="$3"
  [ -f "$relay_in" ] || { echo "relay spec not found: $relay_in" >&2; return 1; }
  [ -f "$paras_json" ] || { echo "paras file not found: $paras_json" >&2; return 1; }
  [ -n "$relay_out" ] || { echo "output path required" >&2; return 1; }
  local tmp_out; tmp_out="$(mktemp_wrk tmp.relayparas.XXXXXX)"
  jq --slurpfile paras "$paras_json" '
    .genesis //= {} |
    .genesis.runtimeGenesis //= {} |
    .genesis.runtimeGenesis.patch //= {} |
    .genesis.runtimeGenesis.patch.paras //= {} |
    .genesis.runtimeGenesis.patch.paras.paras = ((.genesis.runtimeGenesis.patch.paras.paras // []) + $paras[0]) |
    ((.genesis.runtimeGenesis.patch.paras.paras // []) | length) as $cores |
    (if (.genesis.runtimeGenesis.patch.configuration? // {} | .config? // {} | .scheduler_params? // {} | has("num_cores"))
      then .genesis.runtimeGenesis.patch.configuration.config.scheduler_params.num_cores = (if $cores > 0 then $cores else 1 end) else . end) |
    (if (.genesis.runtimeGenesis.patch.configuration? // {} | .config? // {} | has("minimum_backing_votes"))
      then .genesis.runtimeGenesis.patch.configuration.config.minimum_backing_votes = 2 else . end) |
    (if (.genesis.runtimeGenesis.patch.configuration? // {} | .config? // {} | has("needed_approvals"))
      then .genesis.runtimeGenesis.patch.configuration.config.needed_approvals = 2 else . end)
  ' "$relay_in" > "$tmp_out" && mv -- "$tmp_out" "$relay_out"
  echo "Relay spec patched with parachain data → $relay_out"
}

print_validator_run_command() {
  local validator_name="$1"
  [ -n "$validator_name" ] || { echo "Usage: print_run_command <Name>" >&2; return 1; }
  local lower node_dir base_path manifest spec_json chain_id listen_addr rpc_port prom_port node_key_hex p2p_file p2p_note=""
  lower="$(echo "$validator_name" | tr '[:upper:]' '[:lower:]')"
  node_dir="$WORKDIR/nodes/$lower"; manifest="$node_dir/manifest.json"; base_path="$node_dir/base"
  [ -f "$manifest" ] || { echo "ERROR: manifest not found for $validator_name at $manifest" >&2; return 1; }
  if [ -f "$WORKDIR/relaychain-raw.json" ]; then spec_json="$WORKDIR/relaychain-raw.json"; else echo "ERROR: spec json not found (expected $WORKDIR/relaychain-raw.json)" >&2; return 1; fi
  chain_id="$(jq -r '.id // empty' "$spec_json")"; [ -n "$chain_id" ] || { echo "ERROR: .id not found in $spec_json" >&2; return 1; }
  listen_addr="$(jq -r '.listen_address // empty' "$manifest")"
  rpc_port="$(jq -r '.rpc_port // empty' "$manifest")"
  prom_port="$(jq -r '.prometheus_port // empty' "$manifest")"
  node_key_hex="$(jq -r '.node_key // empty' "$manifest")"
  [ -n "$listen_addr" ] && [ -n "$rpc_port" ] && [ -n "$prom_port" ] && [ -n "$node_key_hex" ] || { echo "ERROR: manifest missing listen_address/node_key/rpc_port/prometheus_port" >&2; return 1; }
  p2p_file="$base_path/chains/$chain_id/network/secret_ed25519"; [ -f "$p2p_file" ] || p2p_note=" # (warning: p2p secret not found yet; run provision_node_keys)"
  cat <<CMD
SHADOW_TAG="$validator_name" "$POLKADOT_BIN" \\
  --validator \\
  --name "$validator_name" \\
  --base-path "$base_path" \\
  --chain "$spec_json" \\
  --listen-addr "$listen_addr" \\
  --public-addr "$listen_addr" \\
  --node-key "$node_key_hex" \\
  --rpc-port $rpc_port \\
  --rpc-cors all \\
  --rpc-methods unsafe \\
  --prometheus-port $prom_port \\
  --prometheus-external \\
  --no-mdns \\
  --no-telemetry \\
  --no-hardware-benchmarks \\
  --insecure-validator-i-know-what-i-do \\
  -l$LOGCFG > "$validator_name.log" 2>&1 &${p2p_note}
CMD
}

print_collator_run_command() {
  local collator_name="$1" para_spec_json="$2"
  [ -n "$collator_name" ] && [ -n "$para_spec_json" ] && [ -f "$para_spec_json" ] || { echo "Usage: print_collator_run_command <Name> <PARACHAIN_SPEC_JSON>" >&2; return 1; }
  local lower node_dir base_path manifest relay_spec_json
  lower="$(echo "$collator_name" | tr '[:upper:]' '[:lower:]')"
  node_dir="$WORKDIR/nodes/$lower"; base_path="$node_dir/base"; manifest="$node_dir/manifest.json"
  [ -f "$WORKDIR/relaychain-raw.json" ] && relay_spec_json="$WORKDIR/relaychain-raw.json" || relay_spec_json=""

  local listen_addr_para node_key_para peer_id_para
  listen_addr_para="$(jq -r '.network.para.listen_address // .listen_address // empty' "$manifest")"
  node_key_para="$(jq -r '.network.para.node_key     // .node_key       // empty' "$manifest")"

  local listen_addr_relay node_key_relay
  listen_addr_relay="$(jq -r '.network.relay.listen_address // empty' "$manifest")"
  node_key_relay="$(jq -r '.network.relay.node_key       // empty' "$manifest")"

  local rpc_port prom_port
  rpc_port="$(jq -r '.rpc_port // empty' "$manifest")"
  prom_port="$(jq -r '.prometheus_port // empty' "$manifest")"
  [ -n "$listen_addr_para" ] && [ -n "$rpc_port" ] && [ -n "$prom_port" ] && [ -n "$node_key_para" ] || { echo "ERROR: manifest missing listen_address/node_key/rpc_port/prometheus_port" >&2; return 1; }
  force_authoring=""
  if ((COLLATORS == 1)); then
    echo "COLLATORS=$COLLATORS"
    force_authoring="--force-authoring"
  fi

  cat <<CMD
SHADOW_TAG="$collator_name" "$COLLATOR_BIN" \\
  --collator $force_authoring \\
  --name "$collator_name" \\
  --base-path "$base_path" \\
  --chain "$para_spec_json" \\
  --listen-addr "$listen_addr_para" \\
  --public-addr "$listen_addr_para" \\
  --node-key "$node_key_para" \\
  --rpc-port $rpc_port \\
  --rpc-cors all \\
  --rpc-methods unsafe \\
  --prometheus-port $prom_port \\
  --prometheus-external \\
  --no-mdns \\
  --no-telemetry \\
  --no-hardware-benchmarks \\
  -l$LOGCFG \\
  -- \\
  --base-path "$base_path/../relay" \\
  --chain "$relay_spec_json" \\
  --listen-addr "$listen_addr_relay" \\
  --public-addr "$listen_addr_relay" \\
  --node-key "$node_key_relay" \\
  --no-prometheus \\
  --no-mdns \\
  --no-telemetry \\
  --no-hardware-benchmarks \\
  --no-beefy \\
  -l$LOGCFG > "$collator_name.log" 2>&1 &
CMD
}

print_run_commands() {
  for ((v=0; v<VALIDATORS; v++)); do echo; print_validator_run_command "Validator_$((v+1))"; done
  for ((p=0; p<PARACHAINS; p++)); do
    for ((c=0; c<COLLATORS; c++)); do echo; print_collator_run_command "Collator_$((PARA_BASE+p))_$((c+1))" "$WORKDIR/parachain-$((PARA_BASE+p))-raw.json"; done
  done
}

# =====================
# Shadow YAML generator
# =====================
generate_shadow_config() {
  local out="$WORKDIR/shadow.yaml"
  mkdir -p -- "$WORKDIR"

  local host_labels=() host_types=() host_para_ids=() host_coll_idxs=()
  local name lower node_dir manifest
  for ((v=0; v<VALIDATORS; v++)); do
    name="Validator_$((v+1))"; lower="$(lc "$name")"; manifest="$WORKDIR/nodes/$lower/manifest.json"
    if [ -f "$manifest" ]; then
      host_labels+=("$name")
      host_types+=("validator")
      host_para_ids+=(0)
      host_coll_idxs+=(0)
    else
      echo "WARN: manifest missing for $name: $manifest — skipping" >&2
    fi
  done
  for ((p=0; p<PARACHAINS; p++)); do
    for ((c=0; c<COLLATORS; c++)); do
      name="Collator_$((PARA_BASE+p))_$((c+1))"; lower="$(lc "$name")"; manifest="$WORKDIR/nodes/$lower/manifest.json"
      if [ -f "$manifest" ]; then
        host_labels+=("$name")
        host_types+=("collator")
        host_para_ids+=("$((PARA_BASE+p))")
        host_coll_idxs+=("$((c+1))")
      else
        echo "WARN: manifest missing for $name: $manifest — skipping" >&2
      fi
    done
  done
  local total_hosts="${#host_labels[@]}"

  {
    printf 'general:\n'
    printf '  stop_time: "20 min"\n'
    printf '  model_unblocked_syscall_latency: true\n\n'
    printf 'experimental:\n'
    printf '  native_preemption_enabled: true\n'
    printf '  unblocked_syscall_latency: "1 microseconds"\n'
    printf '  report_errors_to_stderr: true\n'
    printf '  socket_send_autotune: true\n'
    printf '  socket_recv_autotune: true\n'
    printf '  socket_send_buffer: "4 MiB"\n'
    printf '  socket_recv_buffer: "4 MiB"\n\n'
    printf 'network:\n'
    printf '  graph:\n'
    printf '    type: gml\n'
    printf '    inline: |\n'
    printf '      graph [\n'
    printf '        directed 0\n'
    local i j
    for ((i=1; i<=total_hosts; i++)); do
      printf '        node [\n'
      printf '          id %d\n' "$i"
      printf '          label "%s"\n' "${host_labels[$((i-1))]}"
      printf '          host_bandwidth_up "%s"\n' "$HOST_BW_UP"
      printf '          host_bandwidth_down "%s"\n' "$HOST_BW_DOWN"
      printf '        ]\n'
    done
    for ((i=1; i<=total_hosts; i++)); do
      printf '        edge [\n'
      printf '          source %d\n' "$i"
      printf '          target %d\n' "$i"
      printf '          latency "%s"\n' "$SHADOW_LATENCY"
      printf '          packet_loss 0.0\n'
      printf '        ]\n'
    done
    for ((i=1; i<=total_hosts; i++)); do
      for ((j=i+1; j<=total_hosts; j++)); do
        # Default packet loss for all links
        pl="0.0"
        if [ "${ISOLATE_COLLATOR_IDX:-0}" -gt 0 ]; then
          # Arrays are 0-based; graph ids are 1-based
          ii=$((i-1)); jj=$((j-1))
          ti="${host_types[$ii]}"; tj="${host_types[$jj]}"
          if [ "$ti" = "collator" ] && [ "$tj" = "collator" ]; then
            pi="${host_para_ids[$ii]}"; pj="${host_para_ids[$jj]}"
            if [ "$pi" -eq "$pj" ]; then
              ci="${host_coll_idxs[$ii]}"; cj="${host_coll_idxs[$jj]}"
              if [ "$ci" -ne "$cj" ] && { [ "$ci" -eq "$ISOLATE_COLLATOR_IDX" ] || [ "$cj" -eq "$ISOLATE_COLLATOR_IDX" ]; }; then
                pl="1.0"
              fi
            fi
          fi
        fi
        printf '        edge [\n'
        printf '          source %d\n' "$i"
        printf '          target %d\n' "$j"
        printf '          latency "%s"\n' "$SHADOW_LATENCY"
        printf '          packet_loss %s\n' "$pl"
        printf '        ]\n'
      done
    done
    printf '      ]\n\n'
    printf 'hosts:\n'
  } >"$out"

  local ip_prefix ip_octet=1 lower base_path listen_addr rpc_port prom_port node_key_hex relay_spec_json host host_key net_id=1
  if [ -n "${USE_LOCALHOST:-}" ]; then ip_prefix="127.0.0"; else ip_prefix="10.0.0"; fi
  [ -f "$WORKDIR/relaychain-raw.json" ] && relay_spec_json="$WORKDIR/relaychain-raw.json" || relay_spec_json=""

  # Validators
  for ((v=0; v<VALIDATORS; v++)); do
    name="Validator_$((v+1))"; lower="$(lc "$name")"; base_path="$WORKDIR/nodes/$lower/base"; manifest="$WORKDIR/nodes/$lower/manifest.json"
    [ -f "$manifest" ] || { echo "WARN: manifest missing for $name: $manifest — skipping" >&2; continue; }
    listen_addr="$(jq -r '.listen_address // empty' "$manifest")"; rpc_port="$(jq -r '.rpc_port // empty' "$manifest")"; prom_port="$(jq -r '.prometheus_port // empty' "$manifest")"; node_key_hex="$(jq -r '.node_key // empty' "$manifest")"
    host="$name"; host_key="$(printf '%s' "$host" | tr '[:upper:]' '[:lower:]' | tr '_' '-')"
    {
      printf '  %s:\n' "$host_key"
      printf '    network_node_id: %d\n' "$net_id"
      printf '    ip_addr: %s.%d\n' "$ip_prefix" "$ip_octet"
      printf '    processes:\n'
      printf '      - path: %s\n' "$POLKADOT_BIN"
      printf '        args: [\n'
      printf '          "--validator",\n'
      printf '          "--name", "%s",\n' "$name"
      printf '          "--base-path", "%s",\n' "$base_path"
      printf '          "--chain", "%s",\n' "$WORKDIR/relaychain-raw.json"
      printf '          "--listen-addr", "%s",\n' "$listen_addr"
      printf '          "--public-addr", "%s",\n' "$listen_addr"
      printf '          "--node-key", "%s",\n' "$node_key_hex"
      printf '          "--rpc-port", "%s",\n' "$rpc_port"
      printf '          "--prometheus-port", "%s",\n' "$prom_port"
      printf '          "--prometheus-external",\n'
      printf '          "--no-mdns",\n'
      printf '          "--no-telemetry",\n'
      printf '          "--no-hardware-benchmarks",\n'
      printf '          "--no-beefy",\n'
      printf '          "--insecure-validator-i-know-what-i-do",\n'
      printf '          "-l%s"\n' "$LOGCFG"
      printf '        ]\n'
      printf '        environment:\n'
      printf '          RUST_BACKTRACE: "1"\n'
      printf '          COLORBT_SHOW_HIDDEN: "1"\n'
      printf '          RUST_STDOUT_FLUSH_ON_WRITE: "1"\n'
      printf '          RUST_LOG: "%s"\n' "$LOGCFG"
      printf '          SHADOW_TAG: "%s"\n' "$host"
      printf '        expected_final_state: running\n'
    } >>"$out"
    ip_octet=$((ip_octet+1)); net_id=$((net_id+1))
  done

  # Collators
  for ((p=0; p<PARACHAINS; p++)); do
    local id=$((PARA_BASE+p)) para_raw="$WORKDIR/parachain-$id-raw.json"
    for ((c=0; c<COLLATORS; c++)); do
      name="Collator_$((PARA_BASE+p))_$((c+1))"; lower="$(lc "$name")"; base_path="$WORKDIR/nodes/$lower/base"; manifest="$WORKDIR/nodes/$lower/manifest.json"
      [ -f "$manifest" ] || { echo "WARN: manifest missing for $name: $manifest — skipping" >&2; continue; }
      # Load both para and relay network values
      listen_addr="$(jq -r '.network.para.listen_address // .listen_address // empty' "$manifest")"
      rpc_port="$(jq -r '.rpc_port // empty' "$manifest")"
      prom_port="$(jq -r '.prometheus_port // empty' "$manifest")"
      node_key_hex="$(jq -r '.network.para.node_key // .node_key // empty' "$manifest")"
      relay_listen_addr="$(jq -r '.network.relay.listen_address // empty' "$manifest")"
      relay_node_key_hex="$(jq -r '.network.relay.node_key // empty' "$manifest")"
      host="$name"; host_key="$(printf '%s' "$host" | tr '[:upper:]' '[:lower:]' | tr '_' '-')"
      {
        printf '  %s:\n' "$host_key"
        printf '    network_node_id: %d\n' "$net_id"
        printf '    ip_addr: %s.%d\n' "$ip_prefix" "$ip_octet"
        printf '    processes:\n'
        printf '      - path: %s\n' "$COLLATOR_BIN"
        printf '        args: [\n'
        printf '          "--collator",\n'
        if ((COLLATORS == 1)); then
          printf '          "--force-authoring",\n'
        fi
        printf '          "--name", "%s",\n' "$name"
        printf '          "--base-path", "%s",\n' "$base_path"
        printf '          "--chain", "%s",\n' "$para_raw"
        printf '          "--listen-addr", "%s",\n' "$listen_addr"
        printf '          "--public-addr", "%s",\n' "$listen_addr"
        printf '          "--node-key", "%s",\n' "$node_key_hex"
        printf '          "--rpc-port", "%s",\n' "$rpc_port"
        printf '          "--prometheus-port", "%s",\n' "$prom_port"
        printf '          "--prometheus-external",\n'
        printf '          "--no-mdns",\n'
        printf '          "--no-telemetry",\n'
        printf '          "--no-hardware-benchmarks",\n'
        printf '          "-l%s",\n' "$LOGCFG"
        printf '          "--",\n'
        printf '          "--base-path", "%s/../relay",\n' "$base_path"
        printf '          "--chain", "%s",\n' "$relay_spec_json"
        printf '          "--listen-addr", "%s",\n' "$relay_listen_addr"
        printf '          "--public-addr", "%s",\n' "$relay_listen_addr"
        printf '          "--node-key", "%s",\n' "$relay_node_key_hex"
        printf '          "--no-prometheus",\n'
        printf '          "--no-mdns",\n'
        printf '          "--no-telemetry",\n'
        printf '          "--no-hardware-benchmarks",\n'
        printf '          "--no-beefy",\n'
        printf '          "-l%s"\n' "$LOGCFG"
        printf '        ]\n'
        printf '        environment:\n'
        printf '          RUST_BACKTRACE: "1"\n'
        printf '          COLORBT_SHOW_HIDDEN: "1"\n'
        printf '          RUST_STDOUT_FLUSH_ON_WRITE: "1"\n'
        printf '          RUST_LOG: "%s"\n' "$LOGCFG"
        printf '          SHADOW_TAG: "%s"\n' "$host"
        printf '        expected_final_state: running\n'
      } >>"$out"
      ip_octet=$((ip_octet+1)); net_id=$((net_id+1))
    done
  done
  echo "Shadow config written: $out"
}

# =====================
# Pipeline
# =====================
for ((v=0; v<VALIDATORS; v++)); do prepare_manifest "$v" "Validator_$((v+1))"; done
for ((p=0; p<PARACHAINS; p++)); do
  for ((c=0; c<COLLATORS; c++)); do
    i=$((VALIDATORS + (p * COLLATORS) + c))
    prepare_manifest "$i" "Collator_$((PARA_BASE+p))_$((c+1))"
  done
done

cp -a "$PARA_SPEC_TMPL" "$WORKDIR/parachain.json"
cp -a "$RELAY_SPEC_TMPL" "$WORKDIR/relaychain.json"

cp "$WORKDIR/relaychain.json" "$WORKDIR/relaychain-val.json"
clean_dev_validators_patch "$WORKDIR/relaychain-val.json"
for ((v=0; v<VALIDATORS; v++)); do add_dev_validators_patch "$WORKDIR/relaychain-val.json" "Validator_$((v+1))"; done

replace_runtime_code "$WORKDIR/relaychain.json" "$WORKDIR/relaychain-no-code.json"
replace_runtime_code "$WORKDIR/relaychain-val.json" "$WORKDIR/relaychain-val-no-code.json"
replace_runtime_code "$WORKDIR/parachain.json" "$WORKDIR/parachain-no-code.json"

paras_file="$WORKDIR/paras.json"; printf '[]' > "$paras_file"

for ((p=0; p<PARACHAINS; p++)); do
  id=$((PARA_BASE+p))
  gfile="$WORKDIR/para-${id}-genesis"; wfile="$WORKDIR/para-${id}-wasm"
  cp "$PARA_SPEC_TMPL" "$WORKDIR/parachain-$id.json"
  clean_dev_collators_patch "$WORKDIR/parachain-$id.json" "$id"
  for ((c=0; c<COLLATORS; c++)); do add_dev_collators_patch "$WORKDIR/parachain-$id.json" "Collator_$((PARA_BASE+p))_$((c+1))"; done
  # Apply glutton PoV parameters (storage, compute, blockLength, trashDataCount)
  # FixedU64 values (storage, compute, blockLength) must be strings
  # u32 value (trashDataCount) must be integer
  tmp_parachain="$(mktemp_wrk tmp.glutton.XXXXXX)"
  jq \
    --arg storage "$GLUTTON_STORAGE" \
    --arg compute "$GLUTTON_COMPUTE" \
    --arg block_length "$GLUTTON_BLOCK_LENGTH" \
    --argjson trash "$GLUTTON_TRASH_DATA_COUNT" '
    .genesis //= {} |
    .genesis.runtimeGenesis //= {} |
    .genesis.runtimeGenesis.patch //= {} |
    .genesis.runtimeGenesis.patch.glutton //= {} |
    .genesis.runtimeGenesis.patch.glutton.storage = $storage |
    .genesis.runtimeGenesis.patch.glutton.compute = $compute |
    .genesis.runtimeGenesis.patch.glutton.blockLength = $block_length |
    .genesis.runtimeGenesis.patch.glutton.trashDataCount = $trash |
    .genesis.runtimeGenesis.patch.glutton |= (del(.block_length) | del(.trash_data_count))
  ' "$WORKDIR/parachain-$id.json" > "$tmp_parachain" && mv -- "$tmp_parachain" "$WORKDIR/parachain-$id.json"

  dbg "Exporting parachain $id genesis state..."
  "$COLLATOR_BIN" export-genesis-state --chain "$WORKDIR/parachain-$id.json" "$gfile"
  dbg "Exporting parachain $id genesis wasm..."
  "$COLLATOR_BIN" export-genesis-wasm  --chain "$WORKDIR/parachain-$id.json" "$wfile"

  tmp_paras="$(mktemp_wrk tmp.paras.XXXXXX)"
  jq --rawfile gh "$gfile" --rawfile vc "$wfile" --argjson id "$id" '. + [[ $id, [ ($gh|gsub("[\r\n]";"")), ($vc|gsub("[\r\n]";"")), true ] ]]' "$paras_file" > "$tmp_paras" && mv -- "$tmp_paras" "$paras_file"

  dbg "Building raw parachain $id spec..."
  "$COLLATOR_BIN" build-spec --chain "$WORKDIR/parachain-$id.json" --disable-default-bootnode --raw > "$WORKDIR/parachain-$id-raw.json"
  replace_runtime_code "$WORKDIR/parachain-$id.json" "$WORKDIR/parachain-$id-no-code.json"
  # Keep parachain-$id.json for debugging (comment out the line below in clean() that removes it)
  dbg "DEBUG: Glutton config in parachain-$id.json:" >&2
  jq '.genesis.runtimeGenesis.patch.glutton' "$WORKDIR/parachain-$id.json" >&2 || echo "No glutton config found" >&2
done

patch_relay_with_paras "$WORKDIR/relaychain-val.json" "$paras_file" "$WORKDIR/relaychain-val-paras.json"
replace_runtime_code "$WORKDIR/relaychain-val-paras.json" "$WORKDIR/relaychain-val-paras-no-code.json"
"$POLKADOT_BIN" build-spec --chain "$WORKDIR/relaychain-val-paras.json" --raw > "$WORKDIR/relaychain-raw.json" 2>/dev/null

for ((v=0; v<VALIDATORS; v++)); do provision_node_keys "Validator_$((v+1))" "$WORKDIR/relaychain-raw.json"; done
for ((p=0; p<PARACHAINS; p++)); do
  for ((c=0; c<COLLATORS; c++)); do id=$((PARA_BASE+p)); provision_node_keys "Collator_$((PARA_BASE+p))_$((c+1))" "$WORKDIR/parachain-$id-raw.json"; done
done

#clean
print_run_commands
generate_shadow_config
echo "Done. Raw specs, node manifests, and Shadow config are under: $WORKDIR"
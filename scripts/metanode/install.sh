#!/usr/bin/env bash
# install.sh: install, upgrade or remove the metanode systemd units (polkadot-validator,
# polkadot-asset-hub-collator, polkadot-people-collator) on this machine. See
# scripts/metanode/README.md.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  install.sh (--bin-dir DIR | --release TAG) [--config FILE [--force-config]] [--network NAME]
             [--uplink-rate RATE] [--enable ROLES] [--restart] [--source REF|URL]
  install.sh --uninstall [--purge --yes]

  --bin-dir DIR      install polkadot, polkadot-prepare-worker, polkadot-execute-worker and
                     polkadot-parachain from DIR
  --release TAG      download them from the GitHub release TAG (polkadot-stable2609 or later)
                     and verify Parity's release signature on each
  --config FILE      install FILE as /etc/polkadot-metanode/metanode.env; the same file can go to
                     every machine. A different existing metanode.env is only replaced with
                     --force-config, which keeps the old one as metanode.env.<date>
  --network NAME     set NETWORK (polkadot, kusama, westend, paseo) in a metanode.env without one;
                     the chains of NETWORK fill in those the file leaves empty
  --uplink-rate RATE set UPLINK_RATE, the uplink's committed rate (e.g. 1gbit), in metanode.env,
                     which reserves bandwidth for the validator; "none" turns the reservation off
  --enable ROLES     start roles now and at every boot: "all" (polkadot-metanode-all.target, every
                     role of the machine) or a comma-separated list, e.g. validator,asset-hub
  --restart          allow replacing binaries while roles run: stop them, swap, start them again
  --source REF|URL   when this script runs on its own (curl ... | sudo bash -s -- ...), take its
                     other files from REF (a tag, branch or commit) of
                     github.com/paritytech/polkadot-sdk, or from URL, a scripts/metanode directory
  --uninstall        stop and disable every role and remove the installed files; configuration
                     in /etc/polkadot-metanode and state in /var/lib/polkadot-<role> are kept, and
                     the roles that were enabled are listed in /etc/polkadot-metanode/enabled-roles
  --purge            with --uninstall and --yes: also delete /etc/polkadot-metanode, the
                     /var/lib/polkadot-<role> directories (including keystores) and the users
EOF
  exit "${1:-2}"
}

die() {
  echo "install.sh: $*" >&2
  exit 1
}

SRC=$(cd "$(dirname "$0")" && pwd)
BIN_DIR=/usr/local/bin
LIB_DIR=/usr/local/lib/polkadot-metanode
UNIT_DIR=/usr/local/lib/systemd/system
ETC_UNIT_DIR=/etc/systemd/system
RUN_UNIT_DIR=/run/systemd/system
# Where a deb or rpm puts the same units and the launcher (/lib is /usr/lib on merged-/usr systems).
PACKAGE_UNIT_DIRS=(/lib/systemd/system /usr/lib/systemd/system)
PACKAGE_BIN_DIR=/usr/bin
CONF_DIR=/etc/polkadot-metanode
# Written by --uninstall: the roles to enable again once a package has taken over.
ENABLED_ROLES_FILE=$CONF_DIR/enabled-roles
MANIFEST=$LIB_DIR/installed-files
BINARIES=(polkadot polkadot-prepare-worker polkadot-execute-worker polkadot-parachain)
ROLES=(validator asset-hub-collator people-collator)
ROLE_UNITS=(polkadot-validator.service polkadot-asset-hub-collator.service polkadot-people-collator.service)
QOS_UNIT=polkadot-metanode-qos.service
UNITS=("${ROLE_UNITS[@]}" polkadot-metanode.target polkadot-metanode-all.target polkadot-metanode.slice
  polkadot.slice "$QOS_UNIT")
# Everything this script needs next to it. Bootstrap mode fetches exactly these.
FILES=(polkadot-metanodectl public-addr config/metanode.env systemd/common.conf systemd/sysusers.conf
  systemd/nftables-qos.conf "${UNITS[@]/#/systemd/}")
# The drop-in that puts the reservation's nft table back when nftables.service flushes the ruleset.
NFTABLES_DROPIN=$UNIT_DIR/nftables.service.d/50-polkadot-metanode-qos.conf
REPO_RAW_URL=https://raw.githubusercontent.com/paritytech/polkadot-sdk
RELEASE_URL=https://github.com/paritytech/polkadot-sdk/releases/download
# Parity's release signing key, as published on docs.polkadot.com.
RELEASE_KEY=90BD75EBBB8E95CB3DA6078F94A4029AB4B35DAE
KEYSERVER=hkps://keyserver.ubuntu.com

args=("$@")
bin_dir=''
release=''
network=''
config=''
uplink_rate=''
enable=''
source=''
force_config=0 restart=0 uninstall=0 purge=0 yes=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bin-dir) bin_dir=${2:?--bin-dir needs a directory}; shift ;;
    --release) release=${2:?--release needs a tag}; shift ;;
    --network) network=${2:?--network needs a name}; shift ;;
    --config) config=${2:?--config needs a file}; shift ;;
    --force-config) force_config=1 ;;
    --uplink-rate) uplink_rate=${2:?--uplink-rate needs a rate, e.g. 1gbit, or none}; shift ;;
    --enable) enable=${2:?--enable needs "all" or a comma-separated list of roles}; shift ;;
    --source) source=${2:?--source needs a git ref or a URL}; shift ;;
    --restart) restart=1 ;;
    --uninstall) uninstall=1 ;;
    --purge) purge=1 ;;
    --yes) yes=1 ;;
    -h | --help) usage 0 ;;
    *) usage ;;
  esac
  shift
done

[ "$(id -u)" = 0 ] || die "must run as root"
[ -d /run/systemd/system ] || die "systemd is not the init system of this machine"

# Bootstrap: run on its own, e.g. piped from curl, this script has none of its other files. Fetch
# them, and itself, from the same place into a stage directory, then run that copy with the same
# arguments. A fleet then needs no checkout: the same one-liner works on every machine.
if [ ! -f "$SRC/polkadot-metanodectl" ] || [ ! -f "$SRC/systemd/common.conf" ]; then
  [ -n "$source" ] || die "the files next to install.sh are missing; run it from a checkout of" \
    "polkadot-sdk, or pass --source with the tag, branch or commit to fetch them from"
  command -v curl >/dev/null || die "curl is required"
  case "$source" in
    http://* | https://*) base=${source%/} ;;
    *) base=$REPO_RAW_URL/$source/scripts/metanode ;;
  esac
  boot=$(mktemp -d -p /var/tmp polkadot-metanode-source.XXXXXX)
  for file in install.sh "${FILES[@]}"; do
    mkdir -p "$boot/$(dirname "$file")"
    curl -fsSL --retry 3 -o "$boot/$file" "$base/$file" || {
      rm -rf "$boot"
      die "cannot download $base/$file"
    }
  done
  chmod 0755 "$boot/install.sh" "$boot/polkadot-metanodectl" "$boot/public-addr"
  echo "install.sh: fetched the metanode files from $base"
  POLKADOT_METANODE_BOOTSTRAP_DIR=$boot exec bash "$boot/install.sh" "${args[@]}"
fi

active_roles() {
  local role
  for role in "${ROLES[@]}"; do
    if systemctl is-active --quiet "polkadot-$role.service"; then echo "$role"; fi
  done
}

# Resolves the role names operators type most.
canonical_role() {
  case "$1" in
    asset-hub) echo asset-hub-collator ;;
    people) echo people-collator ;;
    *) echo "$1" ;;
  esac
}

uninstall() {
  [ "$purge" = 0 ] || [ "$yes" = 1 ] ||
    die "--purge deletes $CONF_DIR, /var/lib/polkadot-<role> (including keystores) and the users; add --yes"
  local role enabled=() running=()
  # Recorded before anything is disabled, so a package installed next can enable the same roles.
  if [ "$(systemctl is-enabled polkadot-metanode-all.target 2>/dev/null)" = enabled ]; then
    enabled=(all)
  else
    for role in "${ROLES[@]}"; do
      if [ "$(systemctl is-enabled "polkadot-$role.service" 2>/dev/null)" = enabled ]; then
        enabled+=("$role")
      fi
    done
  fi
  mapfile -t running < <(active_roles)
  # Stopping the reservation removes its HTB tree and nft table.
  systemctl disable --now "${UNITS[@]}" >/dev/null 2>&1 || true
  if [ -r "$MANIFEST" ]; then
    xargs -d '\n' rm -f <"$MANIFEST"
    rm -f "$MANIFEST"
  fi
  for role in "${ROLES[@]}"; do
    rmdir "$UNIT_DIR/polkadot-$role.service.d" 2>/dev/null || true
  done
  rmdir "$(dirname "$NFTABLES_DROPIN")" 2>/dev/null || true
  find "$LIB_DIR" -depth -type d -empty -delete 2>/dev/null || true
  # `systemctl disable` removed the links it made; drop any other link into $UNIT_DIR (e.g. made by
  # hand), which now dangles and would confuse the units of a package installed next.
  find "$ETC_UNIT_DIR" -xtype l -lname "$UNIT_DIR/*" -delete
  systemctl daemon-reload
  if [ "$purge" = 1 ]; then
    rm -rf "$CONF_DIR"
    for role in "${ROLES[@]}"; do
      rm -rf "/var/lib/polkadot-$role"
      userdel "polkadot-$role" 2>/dev/null || true
      groupdel "polkadot-$role" 2>/dev/null || true
    done
    echo "Removed the metanode units, their configuration, state and users."
  else
    if [ -d "$CONF_DIR" ]; then
      if [ ${#enabled[@]} -gt 0 ]; then printf '%s\n' "${enabled[@]}"; fi >"$ENABLED_ROLES_FILE"
    fi
    echo "Removed the metanode units. Kept $CONF_DIR, /var/lib/polkadot-<role> and the" \
      "polkadot-<role> users; --purge --yes deletes them."
    case "${enabled[*]}" in
      "") echo "No role was enabled." ;;
      all)
        echo "Previously enabled: every role (polkadot-metanode-all.target), recorded in" \
          "$ENABLED_ROLES_FILE. After installing the package run:" \
          "systemctl enable --now polkadot-metanode-all.target"
        ;;
      *)
        echo "Previously enabled: ${enabled[*]}, recorded in $ENABLED_ROLES_FILE. After installing" \
          "the package run: systemctl enable --now ${enabled[*]/#/polkadot-}"
        ;;
    esac
    [ ${#running[@]} -eq 0 ] || echo "Running until now: ${running[*]}."
  fi
}

# The same units can come from two places: this script (under /usr/local) or a deb/rpm (under
# /usr). systemd looks a unit up in /etc/systemd/system and /run/systemd/system first, then in
# /usr/local/lib/systemd/system (used here), then in /usr/lib/systemd/system (systemd.unit(5), "Load
# path"), and /usr/local/bin comes before /usr/bin in PATH. Whichever comes first silently wins, so
# refuse every mix:
# - A unit in /etc or /run would shadow ours. operators/setup.md and docs.polkadot.com have
#   operators hand-write /etc/systemd/system/polkadot-validator.service (and the collators' units),
#   the names of our units. Before the first installation, a drop-in directory in /etc belongs to
#   that old unit too and would apply to ours; afterwards it holds the operator's own overrides.
# - Ours would shadow the units and launcher of a package. A package is upgraded through the
#   package manager, or removed before this script is used.
check_not_shadowed() {
  local unit dir path
  for unit in "${UNITS[@]}"; do
    for dir in "$ETC_UNIT_DIR" "$RUN_UNIT_DIR"; do
      path=$dir/$unit
      if [ -e "$path" ] || [ -L "$path" ]; then
        die "$path exists and would take precedence over the $unit installed here. If it is a" \
          "hand-written unit (e.g. from operators/setup.md or docs.polkadot.com), or a masked or" \
          "edited copy, migrate as described in \"Migrating a setup.md installation\" in" \
          "$SRC/README.md and remove it."
      fi
    done
    path=$ETC_UNIT_DIR/$unit
    if [ ! -e "$MANIFEST" ] && [ -d "$path.d" ]; then
      die "$path.d/ predates this installation and would apply to the new $unit. Review and" \
        "remove it first (\"Migrating a setup.md installation\" in $SRC/README.md)."
    fi
    for dir in "${PACKAGE_UNIT_DIRS[@]}"; do
      path=$dir/$unit
      if [ -e "$path" ] || [ -L "$path" ]; then package_owned "$path"; fi
    done
  done
  path=$PACKAGE_BIN_DIR/polkadot-metanodectl
  if [ -e "$path" ] || [ -L "$path" ]; then package_owned "$path"; fi
}

package_owned() {
  die "$1 is installed by a package (dpkg -S $1 / rpm -qf $1); upgrade through the package," \
    "or remove it before using install.sh. The files installed here would take precedence over" \
    "the package's (\"Moving to the package\" in $SRC/README.md)."
}

if [ "$uninstall" = 1 ]; then
  uninstall
  exit 0
fi
[ "$purge" = 0 ] || die "--purge only goes with --uninstall"
if { [ -n "$bin_dir" ] && [ -n "$release" ]; } || { [ -z "$bin_dir" ] && [ -z "$release" ]; }; then
  usage
fi
[ "$(uname -m)" = x86_64 ] || die "only x86_64 is supported: secure validator mode needs seccomp," \
  "which the PVF sandbox supports on x86_64 only"
case "$network" in
  "" | polkadot | kusama | westend | paseo) ;;
  *) die "unknown network '$network' (expected polkadot, kusama, westend or paseo)" ;;
esac
if [ -n "$config" ]; then
  [ -r "$config" ] || die "--config: cannot read $config"
fi
[ "$force_config" = 0 ] || [ -n "$config" ] || die "--force-config only goes with --config"
case "$uplink_rate" in
  "" | none) ;;
  *) [[ "${uplink_rate,,}" =~ ^[0-9]+(\.[0-9]+)?(mbit|gbit)$ ]] ||
    die "--uplink-rate must look like 1gbit, 2.5gbit or 1000mbit, or be none" ;;
esac
# --enable takes the roles explicitly: defaulting to all of them would start a validator on a
# collator-only machine.
enable_roles=()
if [ "$enable" = all ]; then
  enable_roles=("${ROLES[@]}")
elif [ -n "$enable" ]; then
  IFS=',' read -r -a requested <<<"$enable"
  for role in "${requested[@]}"; do
    role=$(canonical_role "$role")
    case " ${ROLES[*]} " in
      *" $role "*) enable_roles+=("$role") ;;
      *) die "unknown role '$role' in --enable (expected all, or some of: ${ROLES[*]})" ;;
    esac
  done
fi
for tool in systemctl systemd-sysusers curl jq sha256sum; do
  command -v "$tool" >/dev/null || die "$tool is required"
done
check_not_shadowed

# Stage on disk: the binaries are ~500MB, and /tmp is a RAM-backed tmpfs on e.g. Debian 13.
stage=$(mktemp -d -p /var/tmp polkadot-metanode-install.XXXXXX)
trap 'rm -rf "$stage" ${POLKADOT_METANODE_BOOTSTRAP_DIR:+"$POLKADOT_METANODE_BOOTSTRAP_DIR"}' EXIT

# ---- configuration: what metanode.env will be, before anything is installed -----------------------

# env_value <file> <key>: the value the file gives the key, as systemd reads it: the last assignment,
# with a pair of quotes around it removed.
env_value() {
  [ -r "$1" ] || return 0
  sed -n "s/^[[:space:]]*$2[[:space:]]*=[[:space:]]*//p" "$1" | tail -n 1 |
    sed -e 's/[[:space:]]*$//' -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'$/\1/"
}

# set_env_key <file> <key> <value>: sets the key, replacing its line or its commented example.
set_env_key() {
  local file=$1 key=$2 value=$3
  if grep -q "^$key=" "$file"; then
    sed -i "s|^$key=.*|$key=$value|" "$file"
  elif grep -q "^#$key=" "$file"; then
    sed -i "0,/^#$key=.*/s||$key=$value|" "$file"
  else
    echo "$key=$value" >>"$file"
  fi
}

# The configuration file this run will leave in place, staged so that it can be checked first. A new
# machine has none until the network is known: the units cannot start without it.
mkdir -p "$stage/conf"
staged_conf=$stage/conf/metanode.env
current_conf=$CONF_DIR/metanode.env
if [ -n "$config" ]; then
  cp "$config" "$staged_conf"
  if [ -e "$current_conf" ] && ! cmp -s "$config" "$current_conf" && [ "$force_config" = 0 ]; then
    die "$current_conf exists and differs from $config; review the difference, then re-run with" \
      "--force-config to replace it (the old file is kept as metanode.env.<date>)"
  fi
elif [ -e "$current_conf" ]; then
  cp "$current_conf" "$staged_conf"
elif [ -n "$network" ]; then
  cp "$SRC/config/metanode.env" "$staged_conf"
fi
# NETWORK of an existing node must not change by accident: its database belongs to that network.
current_network=$(env_value "$current_conf" NETWORK)
staged_network=$(env_value "$staged_conf" NETWORK)
if [ -n "$network" ]; then
  if [ -n "$staged_network" ] && [ "$staged_network" != "$network" ]; then
    die "the configuration already sets NETWORK=$staged_network; edit it by hand to change it"
  fi
  [ -n "$staged_network" ] || set_env_key "$staged_conf" NETWORK "$network"
fi
NETWORK=$(env_value "$staged_conf" NETWORK)
if [ -n "$current_network" ] && [ -n "$NETWORK" ] && [ "$current_network" != "$NETWORK" ]; then
  die "$current_conf sets NETWORK=$current_network but the new configuration sets NETWORK=$NETWORK;" \
    "the nodes' databases belong to $current_network. Change it by hand if you mean to."
fi
# The chains of NETWORK, wherever the file does not set its own.
if [ -n "$NETWORK" ]; then
  while IFS='=' read -r key value; do
    [ -n "$(env_value "$staged_conf" "$key")" ] || set_env_key "$staged_conf" "$key" "$value"
  done < <(POLKADOT_METANODE_LIB_DIR=$LIB_DIR "$SRC/polkadot-metanodectl" network-env "$NETWORK")
fi
case "$uplink_rate" in
  "") ;;
  none)
    if grep -qs '^UPLINK_RATE=' "$staged_conf"; then set_env_key "$staged_conf" UPLINK_RATE ""; fi
    ;;
  *)
    [ -e "$staged_conf" ] || die "--uplink-rate needs a configuration on a new machine: add --network NAME" \
      "or --config FILE"
    set_env_key "$staged_conf" UPLINK_RATE "$uplink_rate"
    ;;
esac
if [ -e "$staged_conf" ]; then
  "$SRC/polkadot-metanodectl" check-config "$staged_conf" ||
    die "fix the configuration${config:+ in $config} (see above)"
fi
UPLINK_RATE=$(env_value "$staged_conf" UPLINK_RATE)
# The reservation needs tc, nft and a kernel without CVE-2026-53079; refuse before installing.
if [ -n "$UPLINK_RATE" ]; then
  POLKADOT_METANODE_CONF_DIR=$stage/conf "$SRC/polkadot-metanodectl" qos check ||
    die "cannot reserve bandwidth with UPLINK_RATE=$UPLINK_RATE (see above); fix it or pass --uplink-rate none"
fi

# ---- binaries and chain specs ---------------------------------------------------------------------

# Downloads the binaries of a release into the stage and verifies each against Parity's release
# signature, following docs.polkadot.com. (The release's .sha256 files are unsigned, so they would
# add nothing to the signature check.)
download_release() {
  local tag=$1 bin file
  command -v gpg >/dev/null || die "gpg is required for --release"
  export GNUPGHOME=$stage/gnupg
  mkdir -m 0700 "$GNUPGHOME"
  gpg --batch --quiet --keyserver "$KEYSERVER" --recv-keys "$RELEASE_KEY" ||
    die "cannot fetch the release key $RELEASE_KEY from $KEYSERVER"
  for bin in "${BINARIES[@]}"; do
    for file in "$bin" "$bin.asc"; do
      curl -fsSL --retry 3 -o "$stage/$file" "$RELEASE_URL/$tag/$file" ||
        die "cannot download $file of release $tag"
    done
    # Accept only a good signature by the release key itself (or one of its subkeys).
    gpg --batch --status-fd 1 --verify "$stage/$bin.asc" "$stage/$bin" 2>/dev/null |
      awk -v key="$RELEASE_KEY" '$1 == "[GNUPG:]" && $2 == "VALIDSIG" && ($3 == key || $NF == key) { ok = 1 }
        END { exit !ok }' || die "$bin is not signed by the Parity release key $RELEASE_KEY"
  done
}

version_of() {
  local out
  out=$("$1" --version 2>/dev/null) || return 0
  if [[ "$out" =~ ([0-9]+\.[0-9]+\.[0-9]+) ]]; then echo "${BASH_REMATCH[1]}"; fi
}

if [ -n "$release" ]; then
  download_release "$release"
else
  for bin in "${BINARIES[@]}"; do
    [ -f "$bin_dir/$bin" ] || die "$bin_dir/$bin not found"
    cp "$bin_dir/$bin" "$stage/$bin"
  done
fi
chmod 0755 "${BINARIES[@]/#/$stage/}"
node_version=$(version_of "$stage/polkadot")
[ -n "$node_version" ] || die "cannot run $stage/polkadot --version"
for bin in polkadot-prepare-worker polkadot-execute-worker; do
  [ "$(version_of "$stage/$bin")" = "$node_version" ] ||
    die "$bin $(version_of "$stage/$bin") does not match polkadot $node_version"
done
# The polkadot package can stay installed next to these binaries; say which one runs where.
if [ -x "$PACKAGE_BIN_DIR/polkadot" ]; then
  echo "install.sh: warning: the polkadot package's $PACKAGE_BIN_DIR/polkadot" \
    "($(version_of "$PACKAGE_BIN_DIR/polkadot")) is installed too. $BIN_DIR/polkadot ($node_version)" \
    "comes first in PATH, and the package's polkadot.service keeps running" \
    "$PACKAGE_BIN_DIR/polkadot; polkadot-validator.service conflicts with it, so only one runs." >&2
fi

# Chain specs the binaries do not build in (Paseo's), downloaded and checked against the sha256s
# pinned in polkadot-metanodectl.
spec_files=()
if [ -n "$NETWORK" ]; then
  "$SRC/polkadot-metanodectl" fetch-specs "$NETWORK" "$stage/chain-specs" >"$stage/spec-files" ||
    die "cannot install the $NETWORK chain specs (see above)"
  mapfile -t spec_files <"$stage/spec-files"
fi

# Replacing binaries under a running validator is not harmless: at its next PVF job the new worker
# detects the version change and shuts the node down, which then waits out RestartSec. So binaries
# only change while no role runs, or with --restart, which restarts the roles deliberately.
changed=()
for bin in "${BINARIES[@]}"; do
  cmp -s "$stage/$bin" "$BIN_DIR/$bin" || changed+=("$bin")
done
mapfile -t running < <(active_roles)
if [ ${#changed[@]} -gt 0 ] && [ ${#running[@]} -gt 0 ] && [ "$restart" = 0 ]; then
  die "new binaries (${changed[*]}) but these roles are running: ${running[*]}." \
    "Stop them first, or re-run with --restart."
fi

# ---- install ----------------------------------------------------------------------------------------

installed=()
# Replaces dst atomically, so a running process keeps the file it opened.
install_file() {
  local mode=$1 src=$2 dst=$3
  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst.metanode-new"
  chmod "$mode" "$dst.metanode-new"
  mv -f "$dst.metanode-new" "$dst"
}

# The same for a unit template, with its placeholders filled in for this layout: the PVF workers go
# next to polkadot.
install_unit() {
  local src=$1 dst=$2
  mkdir -p "$(dirname "$dst")"
  sed -e "s|@BINDIR@|$BIN_DIR|g" -e "s|@WORKERSDIR@|$BIN_DIR|g" -e "s|@LIBDIR@|$LIB_DIR|g" "$src" \
    >"$dst.metanode-new"
  chmod 0644 "$dst.metanode-new"
  mv -f "$dst.metanode-new" "$dst"
}

track() {
  installed+=("$1")
}

restart_roles=()
if [ ${#changed[@]} -gt 0 ] && [ ${#running[@]} -gt 0 ]; then
  restart_roles=("${running[@]}")
  echo "Stopping ${restart_roles[*]} to replace ${changed[*]}"
  systemctl stop "${restart_roles[@]/#/polkadot-}"
fi

for bin in "${BINARIES[@]}"; do
  if [[ " ${changed[*]} " == *" $bin "* ]]; then
    install_file 0755 "$stage/$bin" "$BIN_DIR/$bin"
  fi
  track "$BIN_DIR/$bin"
done
install_file 0755 "$SRC/polkadot-metanodectl" "$BIN_DIR/polkadot-metanodectl"
track "$BIN_DIR/polkadot-metanodectl"
install_file 0755 "$SRC/public-addr" "$LIB_DIR/public-addr"
track "$LIB_DIR/public-addr"
install_file 0644 "$SRC/systemd/sysusers.conf" "$LIB_DIR/sysusers.conf"
track "$LIB_DIR/sysusers.conf"
for file in "${spec_files[@]}"; do
  install_file 0644 "$stage/chain-specs/$file" "$LIB_DIR/chain-specs/$file"
  track "$LIB_DIR/chain-specs/$file"
done
for unit in "${UNITS[@]}"; do
  install_unit "$SRC/systemd/$unit" "$UNIT_DIR/$unit"
  track "$UNIT_DIR/$unit"
done
# The common settings, as a drop-in of each role's unit (see systemd/common.conf for why not a
# prefix drop-in).
for unit in "${ROLE_UNITS[@]}"; do
  install_file 0644 "$SRC/systemd/common.conf" "$UNIT_DIR/$unit.d/10-common.conf"
  track "$UNIT_DIR/$unit.d/10-common.conf"
done
# A no-op unless polkadot-metanode-qos.service is active.
install_unit "$SRC/systemd/nftables-qos.conf" "$NFTABLES_DROPIN"
track "$NFTABLES_DROPIN"
# A previous installation's files that this one no longer has (e.g. another network's chain specs).
if [ -r "$MANIFEST" ]; then
  while IFS= read -r old; do
    [[ " ${installed[*]} " == *" $old "* ]] || rm -f "$old"
  done <"$MANIFEST"
fi

# Configuration belongs to the operator: refresh only the example. metanode.env is the staged one:
# the existing file, or --config's, with --network and --uplink-rate applied.
mkdir -p "$CONF_DIR"
# Left by an earlier --uninstall for a package to read; this installation supersedes it.
rm -f "$ENABLED_ROLES_FILE"
install_file 0644 "$SRC/config/metanode.env" "$CONF_DIR/metanode.env.example"
track "$CONF_DIR/metanode.env.example"
if [ -e "$staged_conf" ] && ! cmp -s "$staged_conf" "$current_conf"; then
  if [ -e "$current_conf" ]; then
    cp -p "$current_conf" "$current_conf.$(date +%Y%m%d-%H%M%S)"
  fi
  install_file 0644 "$staged_conf" "$current_conf"
fi

printf '%s\n' "${installed[@]}" >"$MANIFEST"
systemd-sysusers "$LIB_DIR/sysusers.conf"
systemctl daemon-reload
systemctl enable --quiet polkadot-metanode.target

# The bandwidth reservation follows UPLINK_RATE. Applied before the roles start, and re-applied when
# the rate changed.
if [ -n "$UPLINK_RATE" ]; then
  systemctl enable --quiet "$QOS_UNIT"
  systemctl restart "$QOS_UNIT" || die "$QOS_UNIT failed; see journalctl -u $QOS_UNIT"
elif systemctl is-enabled --quiet "$QOS_UNIT" 2>/dev/null || systemctl is-active --quiet "$QOS_UNIT"; then
  systemctl disable --now --quiet "$QOS_UNIT"
  echo "UPLINK_RATE is not set: removed the bandwidth reservation."
fi

if [ ${#restart_roles[@]} -gt 0 ]; then
  # The validator first: systemd orders the collators after it, and restarts one that started
  # before its RPC answered.
  echo "Starting ${restart_roles[*]}"
  if [[ " ${restart_roles[*]} " == *" validator "* ]]; then
    systemctl start polkadot-validator.service
  fi
  for role in "${restart_roles[@]}"; do
    [ "$role" = validator ] || systemctl start "polkadot-$role.service"
  done
fi

if [ -n "$enable" ]; then
  [ -e "$current_conf" ] || die "$current_conf does not exist, so the roles cannot start: re-run" \
    "with --network NAME or --config FILE"
  if [ "$enable" = all ]; then
    systemctl enable --now --quiet polkadot-metanode-all.target
  else
    systemctl enable --quiet "${enable_roles[@]/#/polkadot-}"
    systemctl start polkadot-metanode.target
  fi
fi

echo "Installed the metanode units with polkadot $node_version (changed: ${changed[*]:-none})."
# What still needs the operator's attention on this machine; it never fails the installation.
"$BIN_DIR/polkadot-metanodectl" doctor "${enable_roles[@]}" || true
if [ -z "$enable" ]; then
  if [ -e "$current_conf" ]; then
    echo "Next: enable the roles of this machine, e.g."
  else
    echo "Next: re-run install.sh with --network NAME (it writes $current_conf and downloads the" \
      "chain specs the network needs), then enable the roles of this machine, e.g."
  fi
  echo "  systemctl enable --now polkadot-metanode-all.target   # every role"
  echo "  systemctl enable --now polkadot-validator polkadot-asset-hub-collator   # or some of them"
  echo "  polkadot-metanodectl status"
fi

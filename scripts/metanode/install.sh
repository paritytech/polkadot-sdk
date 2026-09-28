#!/usr/bin/env bash
# install.sh: install, upgrade or remove the metanode systemd units (polkadot-validator,
# polkadot-asset-hub, polkadot-people) on this machine. See scripts/metanode/README.md.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  install.sh (--bin-dir DIR | --release TAG) [--network NAME] [--enable ROLES] [--restart]
  install.sh --uninstall [--purge --yes]

  --bin-dir DIR    install polkadot, polkadot-prepare-worker, polkadot-execute-worker and
                   polkadot-parachain from DIR
  --release TAG    download them from the GitHub release TAG (polkadot-stable2606 or later, e.g.
                   polkadot-stable2606-2) and verify Parity's release signature on each
  --network NAME   set NETWORK (polkadot, kusama, westend, paseo) in a fresh metanode.conf
  --enable ROLES   start roles now and at every boot: "all" (polkadot-metanode-all.target, every
                   role of the machine) or a comma-separated list, e.g. validator,asset-hub
  --restart        allow replacing binaries while roles run: stop them, swap, start them again
  --uninstall      stop and disable every role and remove the installed files; configuration
                   in /etc/polkadot-metanode and state in /var/lib/polkadot-<role> are kept, and
                   the roles that were enabled are listed in /etc/polkadot-metanode/enabled-roles
  --purge          with --uninstall and --yes: also delete /etc/polkadot-metanode, the
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
ROLES=(validator asset-hub people)
UNITS=(polkadot-validator.service polkadot-asset-hub.service polkadot-people.service
  polkadot-metanode.target polkadot-metanode-all.target polkadot-metanode.slice)
RELEASE_URL=https://github.com/paritytech/polkadot-sdk/releases/download
# Parity's release signing key, as published on docs.polkadot.com.
RELEASE_KEY=90BD75EBBB8E95CB3DA6078F94A4029AB4B35DAE
KEYSERVER=hkps://keyserver.ubuntu.com

bin_dir=''
release=''
network=''
enable=''
restart=0 uninstall=0 purge=0 yes=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bin-dir) bin_dir=${2:?--bin-dir needs a directory}; shift ;;
    --release) release=${2:?--release needs a tag}; shift ;;
    --network) network=${2:?--network needs a name}; shift ;;
    --enable) enable=${2:?--enable needs "all" or a comma-separated list of roles}; shift ;;
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

active_roles() {
  local role
  for role in "${ROLES[@]}"; do
    if systemctl is-active --quiet "polkadot-$role.service"; then echo "$role"; fi
  done
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
  systemctl disable --now "${UNITS[@]}" >/dev/null 2>&1 || true
  if [ -r "$MANIFEST" ]; then
    xargs -d '\n' rm -f <"$MANIFEST"
    rm -f "$MANIFEST"
  fi
  for role in "${ROLES[@]}"; do
    rmdir "$UNIT_DIR/polkadot-$role.service.d" 2>/dev/null || true
  done
  rmdir "$LIB_DIR" 2>/dev/null || true
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
# - A unit in /etc or /run would shadow ours. docs.polkadot.com has operators hand-write
#   /etc/systemd/system/polkadot-validator.service, the name of our validator unit. Before the first
#   installation, a drop-in directory in /etc belongs to that old unit too and would apply to ours;
#   afterwards it holds the operator's own overrides.
# - Ours would shadow the units and launcher of a package. A package is upgraded through the
#   package manager, or removed before this script is used.
check_not_shadowed() {
  local unit dir path
  for unit in "${UNITS[@]}"; do
    for dir in "$ETC_UNIT_DIR" "$RUN_UNIT_DIR"; do
      path=$dir/$unit
      if [ -e "$path" ] || [ -L "$path" ]; then
        die "$path exists and would take precedence over the $unit installed here. If it is a" \
          "hand-written unit (e.g. from docs.polkadot.com), or a masked or edited copy, migrate as" \
          "described in the \"Moving an existing validator\" section of $SRC/README.md and remove it."
      fi
    done
    path=$ETC_UNIT_DIR/$unit
    if [ ! -e "$MANIFEST" ] && [ -d "$path.d" ]; then
      die "$path.d/ predates this installation and would apply to the new $unit. Review and" \
        "remove it first (\"Moving an existing validator\" in $SRC/README.md)."
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
# NETWORK already configured, if any. Checked before anything is installed.
current_network=
if [ -r "$CONF_DIR/metanode.conf" ]; then
  current_network=$(sed -n 's/^NETWORK=//p' "$CONF_DIR/metanode.conf" | tail -n 1)
fi
if [ -n "$network" ] && [ -n "$current_network" ] && [ "$current_network" != "$network" ]; then
  die "$CONF_DIR/metanode.conf already sets NETWORK=$current_network; edit it by hand to change it"
fi
# --enable takes the roles explicitly: defaulting to all of them would start a validator on a
# collator-only machine.
enable_roles=()
if [ "$enable" = all ]; then
  enable_roles=("${ROLES[@]}")
elif [ -n "$enable" ]; then
  IFS=',' read -r -a enable_roles <<<"$enable"
  for role in "${enable_roles[@]}"; do
    case " ${ROLES[*]} " in
      *" $role "*) ;;
      *) die "unknown role '$role' in --enable (expected all, or some of: ${ROLES[*]})" ;;
    esac
  done
fi
for tool in systemctl systemd-sysusers curl jq; do
  command -v "$tool" >/dev/null || die "$tool is required"
done
check_not_shadowed

# Stage on disk: the binaries are ~500MB, and /tmp is a RAM-backed tmpfs on e.g. Debian 13.
stage=$(mktemp -d -p /var/tmp polkadot-metanode-install.XXXXXX)
trap 'rm -rf "$stage"' EXIT

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

installed=()
# Replaces dst atomically, so a running process keeps the file it opened.
install_file() {
  local mode=$1 src=$2 dst=$3
  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst.metanode-new"
  chmod "$mode" "$dst.metanode-new"
  mv -f "$dst.metanode-new" "$dst"
}

# The same for a unit template, with its @BINDIR@ placeholder filled in for this layout.
install_unit() {
  local src=$1 dst=$2
  mkdir -p "$(dirname "$dst")"
  sed "s|@BINDIR@|$BIN_DIR|g" "$src" >"$dst.metanode-new"
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
install_file 0644 "$SRC/systemd/sysusers.conf" "$LIB_DIR/sysusers.conf"
track "$LIB_DIR/sysusers.conf"
for unit in "${UNITS[@]}"; do
  install_unit "$SRC/systemd/$unit" "$UNIT_DIR/$unit"
  track "$UNIT_DIR/$unit"
done
# The common settings, as a drop-in of each role's unit (see systemd/common.conf for why not a
# prefix drop-in).
for role in "${ROLES[@]}"; do
  install_file 0644 "$SRC/systemd/common.conf" "$UNIT_DIR/polkadot-$role.service.d/10-common.conf"
  track "$UNIT_DIR/polkadot-$role.service.d/10-common.conf"
done

# Configuration belongs to the operator: create it once, refresh only the examples.
mkdir -p "$CONF_DIR"
# Left by an earlier --uninstall for a package to read; this installation supersedes it.
rm -f "$ENABLED_ROLES_FILE"
for conf in metanode validator asset-hub people; do
  install_file 0644 "$SRC/config/$conf.conf" "$CONF_DIR/$conf.conf.example"
  track "$CONF_DIR/$conf.conf.example"
  [ -e "$CONF_DIR/$conf.conf" ] || install_file 0644 "$SRC/config/$conf.conf" "$CONF_DIR/$conf.conf"
done
if [ -n "$network" ] && [ -z "$current_network" ]; then
  sed -i "s/^NETWORK=.*/NETWORK=$network/" "$CONF_DIR/metanode.conf"
fi

printf '%s\n' "${installed[@]}" >"$MANIFEST"
systemd-sysusers "$LIB_DIR/sysusers.conf"
systemctl daemon-reload
systemctl enable --quiet polkadot-metanode.target

if [ ${#restart_roles[@]} -gt 0 ]; then
  # The validator first. Collators wait for its RPC themselves (polkadot-metanodectl), and systemd
  # orders them after it.
  echo "Starting ${restart_roles[*]}"
  if [[ " ${restart_roles[*]} " == *" validator "* ]]; then
    systemctl start polkadot-validator.service
  fi
  for role in "${restart_roles[@]}"; do
    [ "$role" = validator ] || systemctl start "polkadot-$role.service"
  done
fi

if [ -n "$enable" ]; then
  for role in "${enable_roles[@]}"; do
    "$BIN_DIR/polkadot-metanodectl" print "$role" >/dev/null ||
      die "fix $CONF_DIR before enabling $role (see the error above)"
  done
  if [ "$enable" = all ]; then
    systemctl enable --now --quiet polkadot-metanode-all.target
  else
    systemctl enable --quiet "${enable_roles[@]/#/polkadot-}"
    systemctl start polkadot-metanode.target
  fi
fi

echo "Installed the metanode units with polkadot $node_version (changed: ${changed[*]:-none})."
if [ -z "$enable" ]; then
  if grep -qE '^NETWORK=.+' "$CONF_DIR/metanode.conf"; then
    echo "Next: enable the roles of this machine, e.g."
  else
    echo "Next: set NETWORK in $CONF_DIR/metanode.conf, then enable the roles of this machine, e.g."
  fi
  echo "  systemctl enable --now polkadot-metanode-all.target        # every role"
  echo "  systemctl enable --now polkadot-validator polkadot-asset-hub  # or some of them"
  echo "  polkadot-metanodectl status"
fi

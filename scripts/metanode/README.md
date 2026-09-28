# Metanode systemd units

Run a Polkadot relay-chain validator and system-parachain collators (Asset Hub, People) as systemd
services, one unit per role. Any machine can run any subset of the roles:

```sh
# machine A: every role, the collators sharing the validator's relay-chain RPC
systemctl enable --now polkadot-metanode-all.target

# machine B: some roles only, e.g. a collator using the validator on machine A
systemctl enable --now polkadot-asset-hub
```

| Unit | Role | Binary |
|---|---|---|
| `polkadot-validator.service` | relay-chain validator | `polkadot --validator` |
| `polkadot-asset-hub.service` | Asset Hub collator | `polkadot-parachain --collator` |
| `polkadot-people.service` | People collator | `polkadot-parachain --collator` |
| `polkadot-metanode.target` | the roles enabled one by one on the machine | |
| `polkadot-metanode-all.target` | every role, without enabling each | |

Enable either `polkadot-metanode-all.target` (every role) or the roles a machine should run.
`systemctl start|stop|restart polkadot-metanode.target` acts on every enabled role, and stopping or
restarting `polkadot-metanode-all.target` acts on all of them. Each role runs as its own system user
(`polkadot-<role>`), so a collator cannot read the validator's keystore. State lives in
`/var/lib/polkadot-<role>`.

## Why metanode

The plan "Metanode – plan and phasing" makes validators also collate for the system parachains:
Asset Hub first, then People and the other system chains. Their collator selection would also
accept validators that have set their collator session keys, so the validator set collates the
system chains too. Block production then shows whether a validator runs its collators well; rewards
or penalties can follow.

A *metanode* is what one operator runs for that: the validator plus its system-chain collators, on
one machine or spread over several, and later the other services named in the plan (TURN/STUN,
notifications, SSS). A collator does not need its own relay-chain node; it can use its operator's
validator over RPC.

Validators should not have to hand-roll this before they are asked to run it, so the plan defines
supported ("blessed") setups: a docker image, these systemd units, and Helm charts. All of them
share one role table (`polkadot-metanodectl`), so a role has the same binary, ports and flags
everywhere.

The units use standard `polkadot-` names. `polkadot-validator.service` is also the name
docs.polkadot.com suggests for a hand-written validator unit in `/etc/systemd/system`, which would
take precedence over the one installed here, so `install.sh` refuses to install while such a file
exists; see [Moving an existing validator](#moving-an-existing-validator).

## Install

Requirements: x86_64 Linux with systemd (tested on Ubuntu 22.04 and 24.04, Debian 12 and 13),
`curl`, `jq`, and for `--release` also `gpg`; binaries of polkadot-stable2606 or later (the units
pass `--force-enable-webrtc`, see [Light clients](#light-clients)). Run as root from a checkout of
this repository:

```sh
# Download a release, verify Parity's release signature on every binary, and install:
scripts/metanode/install.sh --release polkadot-stable2606-2 --network polkadot

# Or install binaries you built or verified yourself:
scripts/metanode/install.sh --bin-dir target/release --network polkadot
```

This installs `polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`, `polkadot-parachain`
and `polkadot-metanodectl` into `/usr/local/bin`, and the units into `/usr/local/lib/systemd/system`,
each with the shared settings as a drop-in (`polkadot-<role>.service.d/10-common.conf`).
`/etc/systemd/system` stays free for your own overrides. It also creates the users and writes the
configuration templates to `/etc/polkadot-metanode/`. Nothing starts until you enable roles, or pass
`--enable all` (every role, through `polkadot-metanode-all.target`) or `--enable validator,asset-hub`
(those roles). There is no default: a collator-only machine must never start a validator by
accident.

### Layouts

The same units will also come as a deb/rpm package (planned). The two differ only in where the
files go:

| | `install.sh` | package |
|---|---|---|
| `polkadot`, `polkadot-parachain`, `polkadot-metanodectl` | `/usr/local/bin` | `/usr/bin` |
| PVF workers (`polkadot-{prepare,execute}-worker`) | `/usr/local/bin` | `/usr/lib/polkadot` (the `polkadot` package's) |
| units and their `10-common.conf` drop-ins | `/usr/local/lib/systemd/system` | `/lib/systemd/system` (deb), `/usr/lib/systemd/system` (rpm) |
| `sysusers.conf` | `/usr/local/lib/polkadot-metanode` | `/usr/lib/sysusers.d` |

Shared by both: the configuration in `/etc/polkadot-metanode/`, the state in
`/var/lib/polkadot-<role>`, the `polkadot-<role>` users, the unit names and the ports. The unit
files are templates (`ExecStart=@BINDIR@/polkadot-metanodectl run <role>`) that the installer fills
in, and `polkadot-metanodectl` takes the binaries from its own directory. The validator uses the PVF
workers next to `polkadot`, or else those in `/usr/lib/polkadot`.

## Configure

Configuration is bash, read by `polkadot-metanodectl` whenever a role starts; restart a role to apply
a change. Existing files are never overwritten; `install.sh` refreshes `*.conf.example` next to them.

- `/etc/polkadot-metanode/metanode.conf`, shared by every role on the machine:
  - `NETWORK` (required): `polkadot`, `kusama`, `westend` or `paseo`. It selects the relay chain
    and the collators' chains (`asset-hub-<network>`, `people-<network>`).
  - `NODE_NAME`: telemetry name, suffixed with the role. Default: the hostname.
  - `RELAY_CHAIN_RPC_URLS`: see [Collators and the relay chain](#collators-and-the-relay-chain).
  - `PUBLIC_IP`: address to advertise for p2p, needed behind NAT.
  - `BIN_DIR`: where the binaries are. Default: the directory `polkadot-metanodectl` is installed
    in. The validator takes the PVF workers from there, or else from `/usr/lib/polkadot`. A missing
    worker, or one whose version differs from `polkadot`'s, fails the validator with exit status 78.
- `/etc/polkadot-metanode/<role>.conf`, overrides for one role: `CHAIN`, `RELAY_CHAIN`, ports,
  `AUTHORING`, `PROMETHEUS_EXTERNAL`, database and history (`DATABASE`, `STATE_PRUNING`,
  `BLOCKS_PRUNING`, `SYNC`, and `RELAY_*` for an embedded relay node), `EXTRA_ARGS`,
  `RELAY_EXTRA_ARGS`, and for the validator `RPC_PRIVATE_IP`. Each template documents its keys.

Collators default to the settings of the docs.polkadot.com collator guide: ParityDB and 256 blocks
of state and block history, and for an embedded relay node also no transaction pool. The one
exception: an embedded relay node keeps the node's default full sync instead of the guide's fast
sync. While a relay node fast syncs, its best block has no state; the collator's DHT bootnode
discovery queries it, fails, and, being an essential task, stops the node, so systemd restarts it
until a start happens to finish the state download first (polkadot-stable2606). Warp sync
(`RELAY_SYNC=warp`) downloads the state before importing blocks, so it is not affected, but it
needs three relay-chain peers to start.
The validator keeps the node's defaults, as the validator guide sets none. An empty value passes no
flag. `EXTRA_ARGS` may not repeat a flag that is already set from a key; the role then fails with
exit status 78 and says which key to use.

Paseo's system chains are not built into `polkadot-parachain`: download their specs from
<https://paritytech.github.io/chainspecs> and set `CHAIN=/absolute/path.json` in the collator's file.

`polkadot-metanodectl print <role>` shows the exact command line a role would run. A configuration
error makes the role fail with exit status 78 (see `systemctl status polkadot-<role>`) instead of
restarting in a loop.

### Ports

Every role has fixed ports, so any combination fits on one machine. They are the role table of
`polkadot-metanodectl`. Open the p2p ports in your firewall; keep RPC and Prometheus closed unless
noted.

| Role | p2p | relay p2p | RPC | relay RPC | Prometheus | relay Prometheus |
|---|---|---|---|---|---|---|
| validator | 30333 | - | 9944 | - | 9615 | - |
| asset-hub | 30343 | 30344 | 9954 | 9955 | 9625 | 9626 |
| people | 30353 | 30354 | 9964 | 9965 | 9635 | 9636 |

The p2p ports are also used over UDP, for WebRTC (see [Light clients](#light-clients)): open them
for both TCP and UDP. The relay RPC port is only used by an embedded relay node (below), on
localhost. The nodes do not
fail when a port is taken: the p2p layer (litep2p) shares it and RPC moves to a random port. So keep
the table's ports free, and check them with `ss -ltnp`.

## Light clients

Every role serves light clients (e.g. smoldot in a browser) over WebRTC: the units pass
`--force-enable-webrtc`, which makes the node also listen on `/udp/<p2p port>/webrtc-direct`.
Validators and collators do not do that by default. With `PUBLIC_IP` set, the WebRTC address is
advertised too, and the node adds its certificate hash (`/certhash/...`) itself. It needs the
default litep2p network backend and polkadot-stable2606 or later. The relay side of a collator does
not serve WebRTC. `WEBRTC=no` in `metanode.conf` or a `<role>.conf` turns it off.

## Collators and the relay chain

A collator needs a relay-chain node. By default it uses the validator on the same machine over RPC
(`ws://127.0.0.1:9944`), so it runs no relay chain node of its own.

- **Validator elsewhere.** List it in `RELAY_CHAIN_RPC_URLS`, e.g.
  `RELAY_CHAIN_RPC_URLS="ws://10.0.0.5:9944"` (several URLs, space-separated, are tried in order).
  On the validator machine, set `RPC_PRIVATE_IP=10.0.0.5` in `validator.conf`. The validator then
  serves every RPC method on localhost and only the safe ones on that address. Firewall the port to
  your collator machines: the endpoint has no TLS, so use a private network or a tunnel.
- **No validator.** Set `RELAY_CHAIN_RPC_URLS=""`: each collator runs its own embedded relay-chain
  node, which needs roughly twice the disk and CPU.

While no relay-chain RPC answers, a collator waits (`RELAY_RPC_WAIT`, default 300 s) and systemd
retries it, so the start order of machines does not matter.

## Keys

Every role generates its network key on first start (`/var/lib/polkadot-<role>/node-key`, mode
0600), which keeps its peer id across restarts. To generate session keys owned by your stash:

```sh
polkadot-metanodectl rotate-keys validator <stash address>   # submit with stakingRcClient.setKeys on Asset Hub
polkadot-metanodectl rotate-keys asset-hub <stash address>   # submit with session.setKeys on Asset Hub
```

Each call generates new keys. Never copy a keystore to another machine or run two validators with
the same keys: both would sign, which is slashed as equivocation.

## Operate

```sh
polkadot-metanodectl status              # unit state, peers, sync, best/finalized block, peer id
journalctl -u polkadot-validator -f
systemctl restart polkadot-asset-hub
```

The validator waits 120 s before restarting after a crash (GRANDPA votes may not have been
persisted); collators wait 10 s. Roles share the machine through `polkadot-metanode.slice` (under
`polkadot.slice`, as dashes in slice names are hierarchy): the validator has 10 times the CPU and IO
weight of a collator (`IOWeight` needs the BFQ scheduler or io.cost). Tune with
`systemctl edit polkadot-<role>`; such drop-in overrides survive re-installs.

### Without systemd

`polkadot-metanodectl run-all [<role>...]` runs roles (default: all) in the foreground, each output
line prefixed by its role, e.g. in a container or for a quick test. Configuration works as above;
`POLKADOT_METANODE_CONF_DIR` and `POLKADOT_METANODE_BASE_DIR` move the configuration and the state
(`<base>/polkadot-<role>`). There is no supervision and no sandbox: when one node exits, run-all
stops the others and exits with its status; SIGTERM or SIGINT stops them all.

### Upgrade

```sh
scripts/metanode/install.sh --release <new tag> --restart
```

Without `--restart`, `install.sh` refuses to replace binaries while a role runs: the running
validator would stop itself at its next PVF job, when the new worker binaries report a version
mismatch, and then wait out its restart delay. With `--restart` it stops the roles, swaps the
binaries and starts the validator, then the collators.

### Uninstall

`install.sh --uninstall` stops and disables every role and removes the installed files, but keeps
`/etc/polkadot-metanode`, `/var/lib/polkadot-<role>` and the users. It records the roles that were
enabled in `/etc/polkadot-metanode/enabled-roles` (one per line, or `all` for
`polkadot-metanode-all.target`) and prints the command that enables them again. Adding
`--purge --yes` also deletes the configuration, the state (keystores included) and the users.

## Moving to the package

systemd reads `/usr/local/lib/systemd/system` before `/lib/systemd/system` (`systemd.unit(5)`, "Load
path"), and `PATH` has `/usr/local/bin` before `/usr/bin`. With both installed, the `install.sh`
files would silently win over the package's. So the two must never be mixed. `install.sh` refuses
to install while a package's units or `/usr/bin/polkadot-metanodectl` exist; the package must
likewise refuse while an `install.sh` installation exists. To switch:

1. `scripts/metanode/install.sh --uninstall`. The configuration, state and users stay, and
   `/etc/polkadot-metanode/enabled-roles` records the enabled roles.
2. Install the package.
3. Enable the same roles again, with the command the uninstall printed, e.g.
   `systemctl enable --now polkadot-validator polkadot-asset-hub`, or
   `systemctl enable --now polkadot-metanode-all.target` for `all`.

The `polkadot` package alone (its `polkadot.service` and `/usr/bin/polkadot`) may stay installed:
`install.sh` only warns and prints both versions, and `polkadot-validator.service` conflicts with
`polkadot.service`, so only one of them runs.

## Moving an existing validator

Two existing setups are common: the Debian/RPM package's `polkadot.service`, which keeps its data in
`/home/polkadot/.local/share/polkadot`, and a unit written by hand after docs.polkadot.com, usually
`/etc/systemd/system/polkadot-validator.service` with the same data location. `polkadot-validator`
conflicts with `polkadot.service` (starting one stops the other), and `install.sh` refuses to install
while `/etc/systemd/system/polkadot-validator.service` or, on a first installation, its
`polkadot-validator.service.d/` exists, because either would override the unit installed here.

1. Stop and remove the old unit:
   ```sh
   systemctl disable --now polkadot.service              # the package's unit, or:
   systemctl disable --now polkadot-validator.service    # the hand-written one, then
   rm -r /etc/systemd/system/polkadot-validator.service /etc/systemd/system/polkadot-validator.service.d
   systemctl daemon-reload
   ```
2. Install as above, then move (do not copy) the data:
   ```sh
   mkdir -p /var/lib/polkadot-validator
   mv /home/polkadot/.local/share/polkadot/chains /var/lib/polkadot-validator/
   mv /var/lib/polkadot-validator/chains/<chain id>/network/secret_ed25519 /var/lib/polkadot-validator/node-key
   chown -R polkadot-validator:polkadot-validator /var/lib/polkadot-validator
   chmod 0700 /var/lib/polkadot-validator
   ```
3. `systemctl enable --now polkadot-validator` and check that `polkadot-metanodectl status` shows
   the old peer id.

## Security notes

The units apply the sandbox of the packaged `polkadot.service` and add `ProtectHome=`. The validator
additionally allows what the PVF workers need to sandbox themselves: user, mount and other
namespaces, `pivot_root`, landlock and seccomp. At startup the validator checks those features and
logs `Running in Secure Validator Mode` if all is well:

- seccomp is mandatory: without it the validator refuses to start (x86_64 only);
- landlock and the user-namespace/pivot_root sandbox back each other up: if one is missing the
  validator starts and logs it as `Optional: Cannot ...` under `Some security issues have been
  detected`.

**Ubuntu 23.10 and later** restrict unprivileged user namespaces
(`kernel.apparmor_restrict_unprivileged_userns=1`, the default on 24.04). We expect this to make the
user-namespace check fail on such hosts, leaving the validator with landlock and seccomp only; look
for `Cannot unshare user namespace and change root` in `journalctl -u polkadot-validator`. Either
allow user namespaces system-wide (`sysctl kernel.apparmor_restrict_unprivileged_userns=0`, which
weakens that protection for every program), or grant them to the two worker binaries only with an
AppArmor profile such as:

```
# /etc/apparmor.d/polkadot-pvf-workers; load with: apparmor_parser -r /etc/apparmor.d/polkadot-pvf-workers
abi <abi/4.0>,
include <tunables/global>

profile polkadot-pvf-prepare-worker /usr/local/bin/polkadot-prepare-worker flags=(unconfined) {
  userns,
}
profile polkadot-pvf-execute-worker /usr/local/bin/polkadot-execute-worker flags=(unconfined) {
  userns,
}
```

With the package layout, the worker paths are `/usr/lib/polkadot/polkadot-prepare-worker` and
`/usr/lib/polkadot/polkadot-execute-worker`. Neither setting has been verified by the tests here:
containers share the kernel of the host they run on,
so this restriction cannot be reproduced in docker. Restart the validator afterwards and check that
the warning is gone.

Never expose the validator's RPC publicly, and never enable `--rpc-methods unsafe` on a non-local
address.

## Testing

`tests/run.sh` runs everything in docker with systemd as PID 1 (privileged containers):

```sh
cargo build --release -p polkadot -p polkadot-parachain-bin --bin polkadot \
  --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot-parachain
scripts/metanode/tests/run.sh lint                          # golden tests + shellcheck, seconds
scripts/metanode/tests/run.sh lifecycle --distro debian-12  # one distro, ~20 min
scripts/metanode/tests/run.sh e2e                           # four-machine network, ~10 min
scripts/metanode/tests/run.sh all                           # everything
```

`--bin-dir DIR` tests other binaries instead of `target/release`, e.g. the four binaries of a
polkadot-stable2606 (or later) release.

- **lint**: `tests/golden.sh` compares `polkadot-metanodectl print` against
  `tests/golden/*/expected.txt` (regenerate with `UPDATE=1` and review the diff; the launcher's
  directory shows as `@BINDIR@`); shellcheck on every script; no names from before the `polkadot-`
  rename; units and launcher that name no install layout.
- **lifecycle**, per distro: `systemd-analyze verify` and `security`, the installed layout, the scope
  of the common drop-in, then users, permissions, node keys, ports, weights, the PVF sandbox (every
  security check that passes on the host must pass inside the unit), crash and outage recovery,
  `Conflicts=`, both targets, reboot, `run-all`, config errors, the PVF workers of the package
  layout (and a missing or mismatched worker), `rotate-keys`, `status`, re-install, upgrade,
  uninstall (with its `enabled-roles`), the refusal to install under a unit in `/etc` or over a
  package's units or launcher, and the warning next to the `polkadot` package.
- **e2e**: a westend-local relay chain with Asset Hub and People registered at genesis. Alice and Bob
  run validators and collators across four containers on different distros, including collators on
  a remote validator's RPC and one on an embedded relay node. The test checks finality and that both
  collators author, then kills a validator, reboots a collator machine and restarts a target.

Separate runs (e.g. one `lifecycle` per distro) can go in parallel; each cleans up only its own
containers. Containers share the host's kernel and LSMs, so kernel-dependent results (landlock, user
namespaces, AppArmor) describe the host; the harness prints them first. `gen-local-specs.sh` needs
jq >= 1.7 (older versions turn the genesis balances into floats), so the e2e runs it in the
ubuntu-24.04 image.

## TODO

Sharing CPU, memory and networking between the roles of one machine still needs to be done
properly.

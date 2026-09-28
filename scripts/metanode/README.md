# Metanode systemd units

Run a Polkadot relay-chain validator and its system-parachain collators (Asset Hub, People) as
systemd services, in the layout of the metanode operators guide (`operators/setup.md`). Each unit's
`ExecStart` is the guide's command, flag for flag, with the few values that depend on the network
or the machine as variables. `install.sh` does the guide's other steps, plus its CPU, memory and
bandwidth protections, so a machine runs the whole setup after one command. The same configuration
file and the same command work on every machine you run.

| Unit | Role | Binary | User and state |
|---|---|---|---|
| `polkadot-validator.service` | relay-chain validator | `polkadot --validator` | `polkadot-validator`, `/var/lib/polkadot-validator` |
| `polkadot-asset-hub-collator.service` | Asset Hub collator | `polkadot-parachain --collator` | `polkadot-asset-hub-collator`, `/var/lib/polkadot-asset-hub-collator` |
| `polkadot-people-collator.service` | People collator | `polkadot-parachain --collator` | `polkadot-people-collator`, `/var/lib/polkadot-people-collator` |
| `polkadot-metanode.target` | the roles enabled one by one on the machine | | |
| `polkadot-metanode-all.target` | every role, without enabling each | | |
| `polkadot-metanode-qos.service` | uplink bandwidth reserved for the validator | `tc`, `nft` | |

Any machine runs any subset of the roles. Each role runs as its own system user, so a collator
cannot read the validator's keystore, and the bandwidth reservation can tell the validator's
packets apart.

## Quick start: one machine

```sh
git clone --depth 1 https://github.com/paritytech/polkadot-sdk && cd polkadot-sdk
sudo scripts/metanode/install.sh --release polkadot-stable2609 --network polkadot \
  --uplink-rate 1gbit --enable all
polkadot-metanodectl status
```

That downloads the release binaries and checks Parity's signature on each, creates the users,
installs and starts the three roles, reserves 500 Mbit/s of the 1 Gbit/s uplink for the validator,
and ends with `polkadot-metanodectl doctor`, which lists what still needs your attention (open the
p2p ports in the firewall, an NTP client, ...). Then generate session keys with
[`rotate-keys`](#keys), and put a [WSS proxy](#serving-rpc-over-wss) in front of the RPC.

`--uplink-rate` is the uplink's committed rate, the bandwidth your provider guarantees; leave it out
if the validator has its own uplink, or one above 10 Gbit/s ([Bandwidth](#bandwidth)).

## Many machines

`/etc/polkadot-metanode/metanode.env` is the same on every machine; what differs is derived on each
one when a role starts:

- the node names: the short hostname plus the role, e.g. `val-3-validator`;
- the public addresses advertised for p2p and WebRTC: `PUBLIC_IP=auto`, the public source addresses
  of the default routes;
- the network keys: generated at each role's first start;
- the users, the state directories and the ports: fixed per role.

So write the file once and run one command per machine. A file with `NETWORK` alone is enough:
`install.sh` fills in the network's chains. `install.sh` also runs on its own, piped from the
repository, and then fetches its other files from the same tag, branch or commit (`--source`), so
the machines need no checkout:

```sh
cat > metanode.env <<'EOF'
NETWORK=polkadot
UPLINK_RATE=1gbit
EOF
ref=master   # or a tag
for host in val-1 val-2 val-3; do
  scp metanode.env "$host:/tmp/metanode.env"
  ssh "$host" "curl -fsSL https://raw.githubusercontent.com/paritytech/polkadot-sdk/$ref/scripts/metanode/install.sh |
    sudo bash -s -- --source $ref --release polkadot-stable2609 --config /tmp/metanode.env --enable all"
done
```

Re-running the same command is safe: it changes nothing that is up to date, and upgrades binaries
only with `--restart` ([Upgrade](#upgrade)). `--config` refuses to replace a different
`metanode.env` unless `--force-config` is given, which keeps the old one as `metanode.env.<date>`.

**One metanode over several machines.** Put the validator on one machine and its collators on
others with the same file: list the validator's private address in it, and enable different roles
per machine.

```sh
# metanode.env, on every machine
NETWORK=polkadot
RELAY_CHAIN_RPC_URLS=ws://10.0.0.5:9944
```

On the validator's machine (10.0.0.5), `/etc/polkadot-metanode/polkadot-validator.env` sets
`RPC_LISTEN_IP=10.0.0.5`, and `install.sh ... --enable validator`; on the others,
`--enable asset-hub,people`. See [Collators and the relay chain](#collators-and-the-relay-chain).

## Install

Requirements: x86_64 Linux with systemd 249 or later (tested on Ubuntu 22.04, 24.04 and 26.04,
Debian 12 and 13), `curl`, `jq` and `sha256sum`; for `--release` also `gpg`; for the bandwidth
reservation `tc` (iproute2) and `nft` (nftables). Binaries of polkadot-stable2609 or later, the
first release that serves smoldot over WebRTC. Run as root:

```sh
# Download a release, verify Parity's release signature on every binary, and install:
scripts/metanode/install.sh --release polkadot-stable2609 --network polkadot

# Or install binaries you built or verified yourself:
scripts/metanode/install.sh --bin-dir target/release --network polkadot
```

| Option | |
|---|---|
| `--network NAME` | `NETWORK` of a `metanode.env` without one, `polkadot`, `kusama`, `westend` or `paseo`, and its chains |
| `--config FILE` | install `FILE` as `metanode.env` (`--force-config` to replace a different one) |
| `--uplink-rate RATE` | set `UPLINK_RATE` (e.g. `1gbit`) and reserve bandwidth for the validator; `none` stops it |
| `--enable ROLES` | start roles now and at every boot: `all`, or a list such as `validator,asset-hub` |
| `--restart` | replace binaries under running roles: stop them, swap, start them |
| `--source REF\|URL` | run on its own (`curl ... \| bash -s --`): fetch the other files from REF, or a URL |
| `--uninstall [--purge --yes]` | see [Uninstall](#uninstall) |

Nothing starts until you enable roles, by `--enable` or with `systemctl enable --now`. There is no
default: a collator-only machine must never start a validator by accident. Without `--network` or
`--config` there is no `metanode.env`, and the roles cannot start.

It installs `polkadot`, `polkadot-prepare-worker`, `polkadot-execute-worker`, `polkadot-parachain`
and `polkadot-metanodectl` into `/usr/local/bin`; the units into `/usr/local/lib/systemd/system`,
each role with the shared settings as a drop-in (`polkadot-<role>.service.d/10-common.conf`);
`public-addr` and the chain specs the network needs (Paseo's) into
`/usr/local/lib/polkadot-metanode`; and `metanode.env` and its example into `/etc/polkadot-metanode/`.
`/etc/systemd/system` stays free for your own overrides (`systemctl edit polkadot-<role>`), which
survive re-installs.

### Layouts

The same units will also come as a deb/rpm package (planned). The two differ only in where the
files go:

| | `install.sh` | package |
|---|---|---|
| `polkadot`, `polkadot-parachain`, `polkadot-metanodectl` | `/usr/local/bin` | `/usr/bin` |
| PVF workers (`polkadot-{prepare,execute}-worker`) | `/usr/local/bin` | `/usr/lib/polkadot` (the `polkadot` package's) |
| units and their `10-common.conf` drop-ins | `/usr/local/lib/systemd/system` | `/lib/systemd/system` (deb), `/usr/lib/systemd/system` (rpm) |
| `public-addr`, `sysusers.conf`, chain specs | `/usr/local/lib/polkadot-metanode` | `/usr/lib/polkadot-metanode`, `/usr/lib/sysusers.d` |

Shared by both: the configuration in `/etc/polkadot-metanode/`, the state in
`/var/lib/polkadot-<role>`, the `polkadot-<role>` users, the unit names and the ports. The unit
files are templates that the installer fills in: `@BINDIR@` (the binaries), `@WORKERSDIR@` (the
validator's `--workers-path`) and `@LIBDIR@`. `polkadot-metanodectl` finds its own files in
`lib/polkadot-metanode` next to its bin directory.

## Configure

The units are the configuration: `systemctl cat polkadot-<role>` shows exactly what a role runs,
setup.md's command with a few variables. They come, in this order, each overriding the one before,
from:

1. the unit's own `Environment=` line, its defaults;
2. `/etc/polkadot-metanode/metanode.env`, the same on every machine;
3. `/etc/polkadot-metanode/polkadot-<role>.env`, optional, for this machine only.

Both files use systemd's `EnvironmentFile=` syntax: one `KEY=value` per line, quotes optional, a `#`
only starts a comment at the beginning of a line. Restart a role to apply a change, and run
`polkadot-metanodectl doctor` to check: it reads the variables the way systemd does, and reports a
value the nodes would refuse, a flag in `EXTRA_ARGS` that a unit already passes, or a variable no
unit uses.

| Variable | Default | |
|---|---|---|
| `NETWORK` | (required) | `polkadot`, `kusama`, `westend` or `paseo`; `doctor` and `status` check the chains' genesis with it |
| `RELAY_CHAIN`, `ASSET_HUB_CHAIN`, `PEOPLE_CHAIN` | (required) | the roles' `--chain`: a built-in id or the absolute path of a chain spec. `install.sh` fills in those of `NETWORK` |
| `RELAY_CHAIN_RPC_URLS` | `ws://127.0.0.1:9944` | the collators' relay chain RPC, in order of preference; empty: an embedded relay chain node |
| `PUBLIC_IP` | `auto` | addresses to advertise for p2p and WebRTC; see [Light clients](#light-clients) |
| `RPC_LISTEN_IP` | `127.0.0.1` | the address of the safe RPC endpoint; an IPv6 one in brackets, e.g. `[::]` |
| `RPC_MAX_CONNECTIONS` | `500` (validator), `200` (collators) | connections the safe RPC endpoint allows |
| `SYNC` | `warp` | the collators' `--sync` |
| `NODE_NAME` | short hostname | telemetry and log name; each role appends `-<role>` |
| `EXTRA_ARGS`, `RELAY_EXTRA_ARGS` | (none) | extra flags for the node, and after `--` for a collator's relay chain node |
| `UPLINK_RATE`, `UPLINK_DEVICE`, `VALIDATOR_UPLINK_MBIT` | (none), default routes' interface, `500` | the [bandwidth reservation](#bandwidth) |

The commented `metanode.env.example` next to `metanode.env` documents each variable. Values that
belong to one machine, such as `RPC_LISTEN_IP`, `PUBLIC_IP` behind NAT, or a `NODE_NAME` other than
the hostname, go in its `polkadot-<role>.env` files.

**Anything else** is a change to the command itself. The node refuses a flag given twice ("the
argument '--state-pruning <PRUNING_MODE>' cannot be used multiple times"), so `EXTRA_ARGS` can add
flags but not change one the unit passes. Replace the unit's command instead:

```sh
systemctl edit polkadot-asset-hub-collator
```
```ini
[Service]
ExecStart=
ExecStart=/usr/local/bin/polkadot-parachain --collator ...the whole command, changed...
```

A drop-in's `Environment=` works for a variable that neither file sets, e.g. `EXTRA_ARGS`; where a
file sets it, the file wins (systemd's `EnvironmentFile=` overrides `Environment=`), so use the
role's `polkadot-<role>.env` instead.

What the commands add to setup.md's:

- **All roles**: the chain variable as `--chain` (setup.md's validator relies on the default,
  Polkadot), the ports setup.md leaves at their defaults, and the public addresses of `PUBLIC_IP`
  for TCP as well as WebRTC, on IPv4 and IPv6.
- **Validator**: `--workers-path`, the directory of the PVF workers it was installed with.
- **Collators**: `--authoring=slot-based`, which runtimes with a relay-parent offset need, and
  `--sync=${SYNC}`, warp by default: a full sync of People ran CPU-bound at about 22 blocks/s, more
  than five hours. Warp sync needs the parachain's head on the relay chain, so a parachain that is
  not registered there (a dev or test chain) needs `SYNC=full`; it stops with `Could not find
  parachain head in relay chain` otherwise. After `--`, the settings of an embedded relay chain node
  (`--rpc-port` to `--pool-limit=0`), which only runs with `RELAY_CHAIN_RPC_URLS` empty; setup.md's
  relay chain node reads the validator's RPC and ignores them.

An invalid value or a repeated flag makes the node exit with a usage error (status 2), which fails
the unit instead of restarting it in a loop: see `systemctl status polkadot-<role>`. The command
line of a running role is in `/proc/<pid>/cmdline`, e.g.
`tr '\0' ' ' </proc/$(systemctl show -p MainPID --value polkadot-validator)/cmdline`.

### Networks and chain specs

| `NETWORK` | Relay chain | Asset Hub | People | Chain specs |
|---|---|---|---|---|
| `polkadot` | Polkadot | Asset Hub Polkadot | People Polkadot | built into the binaries |
| `kusama` | Kusama | Asset Hub Kusama | People Kusama | built into the binaries |
| `westend` | Westend | Asset Hub Westend | People Westend | built into the binaries |
| `paseo` | Paseo | Asset Hub Next (para 1500) | People Next (para 1502) | downloaded by `install.sh` |

Paseo runs the metanode system chains, Asset Hub Next and People Next. Their specs are not built in,
and the binaries' built-in `paseo` is an older network than the live one (genesis `0x77af…`
instead of `0x3740…`). So `install.sh` downloads the three specs and checks each against a sha256
pinned in `polkadot-metanodectl` (the files the metanode test validators run on). The Asset Hub Next
and People Next specs come from the chainspecs repository: its paritytech.github.io copies give the
wrong genesis. A changed upstream file stops the installation until its sha256 is reviewed.

Every chain also has its genesis hash pinned. `doctor` checks that every relay chain RPC endpoint of
the collators serves the relay chain, so a fallback of another network (Paseo has several) is
reported, and `status` and `doctor` compare the running nodes with it. With a chain of your own (a
`RELAY_CHAIN` other than the network's), `doctor` compares the endpoints with the validator on the
machine, or else with each other.

### Ports

The layout of setup.md: every role has fixed ports, so any combination fits on one machine. They
are in the units' commands, and in the role table of `polkadot-metanodectl`, which `doctor`,
`status` and `rotate-keys` use.

| Role | p2p (TCP and UDP) | relay p2p (TCP) | safe RPC | unsafe RPC | Prometheus | relay Prometheus |
|---|---|---|---|---|---|---|
| validator | 30333 | - | 9944 | 9945 | 9615 | - |
| asset-hub-collator | 30334 | 30335 | 9946 | 9947 | 9616 | 9617 |
| people-collator | 30336 | 30337 | 9948 | 9949 | 9618 | 9619 |

Open the p2p ports, TCP and UDP (UDP is WebRTC), and the collators' relay p2p ports, TCP, in the
firewall; behind NAT, forward them. Everything else listens on localhost. An embedded relay chain
node serves RPC on 9950 (Asset Hub) or 9951 (People), on localhost. The nodes do not fail when a
port is taken: litep2p shares it and RPC moves to a random port. `doctor` reports a port that
another program holds.

### RPC

Every node runs setup.md's two RPC servers:

- **safe**, on `RPC_LISTEN_IP:<safe port>` (default `127.0.0.1`), only the safe methods, at most
  `RPC_MAX_CONNECTIONS` (setup.md's 500 for the validator, 200 for collators; a load test of 25
  users already exhausted 200). A WSS proxy forwards to it, and collators on other machines use the
  validator's. The endpoint has no `cors=` setting: with one, the node also filters `Host` headers
  and rejects requests made to the machine's public address.
- **unsafe**, on `127.0.0.1:<unsafe port>`, every method, for `rotate-keys` and `status`. Never
  proxy it.

To serve RPC to collators on other machines, set the validator's `RPC_LISTEN_IP` to a private
address in `polkadot-validator.env`, and firewall the port to those machines: the endpoint has no
TLS.

### Serving RPC over WSS

Browsers (Polkadot.js Apps, smoldot pages) need `wss://`. Put nginx in front of the safe endpoints,
one path per chain, as in setup.md's [Set Up Secure
WebSocket](https://docs.polkadot.com/node-infrastructure/run-a-node/relay-chain/secure-wss/), with
these differences learned on a test host serving thousands of clients:

```nginx
server {
    listen 443 ssl;
    listen [::]:443 ssl;
    ssl_certificate     /etc/nginx/ssl/metanode-rpc.crt;   # with subjectAltName for your names/IPs
    ssl_certificate_key /etc/nginx/ssl/metanode-rpc.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    # The node accepts 15 MB requests; nginx's default of 1 MB would refuse the larger ones.
    client_max_body_size 15m;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "Upgrade";
    proxy_set_header Host $host;
    # 127.0.0.1, not localhost: localhost also resolves to ::1, where the node does not listen.
    location /relay           { proxy_pass http://127.0.0.1:9944/; }
    location /asset-hub       { proxy_pass http://127.0.0.1:9946/; }
    location /people          { proxy_pass http://127.0.0.1:9948/; }
}
```

Each proxied client takes two nginx connections: raise `worker_connections` (768 by default) and
`worker_rlimit_nofile` in `nginx.conf` above twice the clients you expect per worker. The node pings
WebSocket clients every 30 s, so nginx's 60 s proxy timeout does not drop idle ones. A self-signed
certificate needs `subjectAltName` entries for the addresses clients use, or they reject it.

## Light clients

Every role serves light clients (e.g. smoldot in a browser) over WebRTC: the units pass
`--force-enable-webrtc`, which makes the node also listen on `/udp/<p2p port>/webrtc-direct`.
Validators and collators do not by default. The relay side of a collator does not serve WebRTC.

Light clients need the node's public address, with its WebRTC certificate hash (the node adds the
`/certhash/...` itself). Before each role starts, `public-addr` turns `PUBLIC_IP` into its
`--public-addr` flags, one for TCP and one for WebRTC per address:

- `auto` (default): the source addresses of the default routes, IPv4 and IPv6, when they are public.
  On a machine with a public address, nothing to set.
- one or more addresses, e.g. `PUBLIC_IP=203.0.113.7 2001:db8::7`: behind NAT, the public address
  (forward the p2p ports, TCP and UDP). It belongs to one machine, so set it in its
  `polkadot-<role>.env` files.
- `none`: advertise nothing.

`public-addr` runs as root, before the sandbox applies (`ExecStartPre=-+`): `ip` drops its
capabilities with `capset()` when it isn't root, which the units' system call filter forbids. It
logs what it advertises in the role's journal, and with `auto` and a private source address (NAT) a
warning instead; `doctor` reports the same. If it fails, the node starts without public addresses.

## Collators and the relay chain

A collator needs a relay chain node. By default it uses the validator on the same machine, over its
safe RPC (`ws://127.0.0.1:9944`), and runs no relay chain node of its own.

- **Validator elsewhere.** List it in `RELAY_CHAIN_RPC_URLS`, e.g.
  `RELAY_CHAIN_RPC_URLS=ws://10.0.0.5:9944`, and set `RPC_LISTEN_IP=10.0.0.5` in the validator's
  `polkadot-validator.env`.
- **Fallbacks.** Further URLs follow in order of preference, e.g. public endpoints:
  `RELAY_CHAIN_RPC_URLS=ws://127.0.0.1:9944 wss://rpc.example.org`. When the connection fails, the
  collator moves to the next URL. With polkadot-sdk#13525 it also checks the earlier URLs every 5
  minutes and goes back to the first one that is reachable and caught up; without it, it stays on the
  fallback until that one fails. `doctor` checks that each URL serves the relay chain.
- **No validator.** Set `RELAY_CHAIN_RPC_URLS=` (empty): each collator runs its own embedded relay
  chain node, which needs roughly twice the disk and CPU. It keeps ParityDB and 256 blocks of
  history, no transaction pool, and does a full sync: while a relay chain node fast syncs, its best
  block has no state, and the collator's DHT bootnode discovery, an essential task, fails on it and
  stops the node. `RELAY_EXTRA_ARGS=--sync=warp` avoids that but needs three relay chain peers to
  start.

When no relay chain RPC answers, the collator exits and systemd starts it again 10 s later, until
the validator is back, so the start order of machines does not matter.

## Sharing one machine

### CPU and memory

The validator wins when the roles compete (setup.md, "CPU and memory"):

- `CPUWeight=1000` and `IOWeight=1000`: ten times a collator's share while they compete; no effect
  while idle. `IOWeight` needs the BFQ scheduler or io.cost.
- `MemoryLow=32G`: under memory pressure the kernel reclaims from the collators before the first
  32 GiB the validator uses, its minimum. It reserves nothing. It only protects the validator as far
  as every slice above it is protected, so `polkadot.slice` and `polkadot-metanode.slice`, where the
  roles run, have `MemoryLow=32G` too.
- `OOMScoreAdjust=-900` and `ManagedOOMPreference=avoid`: if memory runs out, the kernel and
  systemd-oomd stop a collator rather than the validator.

The collators' memory is not capped: a warp sync of Asset Hub peaks at about 25 GiB. Tune any of it
with `systemctl edit polkadot-<role>` or `systemctl edit polkadot-metanode.slice`.

### Bandwidth

The validator must get at least 500 Mbit/s of the uplink whatever the collators and their light
clients send (`operators/bandwidth.md`; light clients alone reached 2.8 Gbit/s under stress). With
`UPLINK_RATE` set, `polkadot-metanode-qos.service` builds that guarantee on the uplink before the
validator starts:

```
root qdisc 1: htb, default class 1:20
└─ 1:1   rate LINK (95% of UPLINK_RATE)
   ├─ 1:10  validator        rate 500mbit         ceil LINK  prio 0  → fq_codel
   └─ 1:20  everything else  rate LINK - 500mbit  ceil LINK  prio 1  → fq_codel
```

Each class may send 1 ms of its own rate back to back, and 1 ms of `LINK` when it borrows, so
together they never send more than 1 ms of `LINK` at once. An nft table
(`inet polkadot_metanode_qos`) puts the packets of the `polkadot-validator` user, PVF workers
included, in class 1:10. Whatever one class leaves idle the other may use. The reservation follows
nftables, whose `flush ruleset` deletes the table: a drop-in
(`nftables.service.d/50-polkadot-metanode-qos.conf`) puts it back whenever `nftables.service`
starts or reloads, a restart of the firewall restarts the reservation, and stopping the firewall
stops it. So `/etc/nftables.conf` needs no change.

- `polkadot-metanodectl qos status` shows the classes and their counters;
  `journalctl -u polkadot-metanode-qos` the rates.
- It shapes the interface of the default routes. When IPv4 and IPv6 leave through different
  interfaces, it refuses to guess: set `UPLINK_DEVICE` to the one to shape. Only that interface is
  shaped, so the validator's traffic through the other has no guarantee.
- Not supported on Linux 6.19.x and 7.0.0 to 7.0.9 (Ubuntu: before 7.0.0-28), where fq_codel under
  HTB freezes TCP flows once the link is saturated (CVE-2026-53079): the unit refuses to start there.
- Change the rate with `install.sh ... --uplink-rate 10gbit`, or remove the reservation with
  `--uplink-rate none`. If the uplink interface is re-created (e.g. a driver reload),
  `systemctl restart polkadot-metanode-qos`.
- Not needed when the validator has its own uplink, or the uplink is above 10 Gbit/s.

## Preflight checks

`polkadot-metanodectl doctor [<role>...]` checks the machine for the roles enabled on it (or the
ones named): the variables each role's unit passes to its node, the kernel (5.16 or later), the
validator's PVF workers and its sandbox (seccomp, landlock and the AppArmor user-namespace
restriction), clock synchronisation, physical cores, SMT, memory and disk against the hardware
requirements, ports taken by other programs, the public addresses, the relay chain RPC endpoints
and their genesis, the genesis of the running roles, the bandwidth reservation, and needrestart.
Each check prints `OK`, `WARN` or `FAIL`; it exits 1 if any check failed. `install.sh` runs it at
the end, without failing on it.

## Keys

Every role generates its network key on first start (`/var/lib/polkadot-<role>/node.key`, mode
0600, a step before the node in each unit), which keeps its peer id across restarts. To generate
session keys owned by your stash:

```sh
polkadot-metanodectl rotate-keys validator <stash address>   # submit with stakingRcClient.setKeys on Asset Hub
polkadot-metanodectl rotate-keys asset-hub <stash address>   # submit with session.setKeys on Asset Hub
polkadot-metanodectl rotate-keys people <stash address>      # submit with session.setKeys on People
```

Each call generates new keys, through the role's unsafe RPC on localhost. Never copy a keystore to
another machine or run two validators with the same keys: both would sign, which is slashed as
equivocation.

## Operate

```sh
polkadot-metanodectl status   # unit state, peers, sync, best/finalized block, genesis check, peer id
polkadot-metanodectl doctor
journalctl -u polkadot-validator -f
systemctl restart polkadot-asset-hub-collator
```

`systemctl start|stop|restart polkadot-metanode.target` acts on every enabled role, and stopping or
restarting `polkadot-metanode-all.target` acts on all of them. The validator waits 120 s before
restarting after a crash (GRANDPA votes may not have been persisted); collators wait 10 s.

On Ubuntu, unattended-upgrades and needrestart restart services that use an upgraded library,
the roles included. That is normally fine; to restart them at a time of your choosing instead,
exclude them: `echo '$nrconf{override_rc}{qr(^polkadot-)} = 0;' >
/etc/needrestart/conf.d/polkadot-metanode.conf`.

### Upgrade

```sh
scripts/metanode/install.sh --release <new tag> --restart
```

Without `--restart`, `install.sh` refuses to replace binaries while a role runs: the running
validator would stop itself at its next PVF job, when the new worker binaries report a version
mismatch, and then wait out its restart delay. With `--restart` it stops the roles, swaps the
binaries and starts the validator, then the collators.

### Uninstall

`install.sh --uninstall` stops and disables every role and the bandwidth reservation, and removes the
installed files, but keeps `/etc/polkadot-metanode`, `/var/lib/polkadot-<role>` and the users. It
records the roles that were enabled in `/etc/polkadot-metanode/enabled-roles` (one per line, or
`all` for `polkadot-metanode-all.target`) and prints the command that enables them again. Adding
`--purge --yes` also deletes the configuration, the state (keystores included) and the users.

## Migrating a setup.md installation

A machine set up by hand from setup.md already uses these state directories, `node.key` files and
ports, and usually units with these names in `/etc/systemd/system`, which would take precedence over
the ones installed here: `install.sh` refuses to install while they exist.

1. Stop the old units and move them out of the way:
   ```sh
   systemctl disable --now polkadot-validator polkadot-asset-hub-collator polkadot-people-collator
   mkdir -p /root/old-units
   mv /etc/systemd/system/polkadot-{validator,asset-hub-collator,people-collator}.service* /root/old-units/
   systemctl daemon-reload
   ```
2. Install with `--network` and `--enable`, as above. At the first start, systemd re-owns each state
   directory to its role's user (`StateDirectory=`), which takes a while on a large database. Compare
   the old units' commands with `systemctl cat polkadot-<role>`: a value that differs goes into the
   role's `polkadot-<role>.env`, a flag the new command lacks into its `EXTRA_ARGS`, and anything
   else into a replaced `ExecStart` ([Configure](#configure)).
3. Check that `polkadot-metanodectl status` shows the old peer ids.

A validator of the Debian/RPM package (`polkadot.service`, data in
`/home/polkadot/.local/share/polkadot`) moves the same way: `polkadot-validator` conflicts with
`polkadot.service`, so starting one stops the other. Stop and disable it, install, then move (do
not copy) the data before enabling the validator:

```sh
mkdir -p /var/lib/polkadot-validator
mv /home/polkadot/.local/share/polkadot/chains /var/lib/polkadot-validator/
mv /var/lib/polkadot-validator/chains/<chain id>/network/secret_ed25519 /var/lib/polkadot-validator/node.key
chown -R polkadot-validator:polkadot-validator /var/lib/polkadot-validator
chmod 0700 /var/lib/polkadot-validator
```

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
   `systemctl enable --now polkadot-validator polkadot-asset-hub-collator`, or
   `systemctl enable --now polkadot-metanode-all.target` for `all`.

The `polkadot` package alone (its `polkadot.service` and `/usr/bin/polkadot`) may stay installed:
`install.sh` only warns and prints both versions, and `polkadot-validator.service` conflicts with
`polkadot.service`, so only one of them runs.

## Security notes

The units apply the sandbox of the packaged `polkadot.service` and add `ProtectHome=`. The validator
additionally allows what the PVF workers need to sandbox themselves: user, mount and other
namespaces, `pivot_root`, landlock and seccomp. At startup the validator checks those features and
logs `Running in Secure Validator Mode` if all is well:

- seccomp is mandatory: without it the validator refuses to start (x86_64 only);
- landlock and the user-namespace/pivot_root sandbox back each other up: if one is missing the
  validator starts and logs it as `Optional: Cannot ...` under `Some security issues have been
  detected`.

Only `public-addr` runs outside the sandbox, as root: it reads the routes and writes one line to the
role's runtime directory.

**Ubuntu 23.10 and later** restrict unprivileged user namespaces
(`kernel.apparmor_restrict_unprivileged_userns=1`, the default since 24.04). This makes the
user-namespace check fail on such hosts, leaving the validator with landlock and seccomp only; look
for `Cannot unshare user namespace and change root` in `journalctl -u polkadot-validator`, or run
`doctor`. Either allow user namespaces system-wide (`sysctl
kernel.apparmor_restrict_unprivileged_userns=0`, which weakens that protection for every program),
or grant them to the two worker binaries only with an AppArmor profile such as:

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
containers share the kernel of the host they run on, so this restriction cannot be reproduced in
docker. Restart the validator afterwards and check that the warning is gone.

Never expose the unsafe RPC endpoints, and never proxy them.

## Testing

`tests/run.sh` runs everything in docker with systemd as PID 1 (privileged containers):

```sh
cargo build --release -p polkadot -p polkadot-parachain-bin --bin polkadot \
  --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot-parachain
scripts/metanode/tests/run.sh lint                          # shellcheck and consistency, seconds
scripts/metanode/tests/run.sh units --distro ubuntu-24.04   # the units' command lines, a minute
scripts/metanode/tests/run.sh lifecycle --distro debian-12  # one distro, ~45 min
scripts/metanode/tests/run.sh e2e                           # four-machine network, ~15 min
scripts/metanode/tests/run.sh all                           # everything
```

`--bin-dir DIR` tests other binaries instead of `target/release`, e.g. the four binaries of a
polkadot-stable2609 (or later) release. `lint` and `units` need no binaries.

- **lint**: shellcheck on every script; no names from before the renames; units and tools that name
  no install layout; the units' ports and chain variables match the role table of
  `polkadot-metanodectl`, and the variables the units use match its list and
  `config/metanode.env`.
- **units**, per distro, with stand-in binaries (`tests/stub-node`) that record their command line:
  for each case under `tests/units/` (a `metanode.env`, `polkadot-<role>.env` files, the default
  routes), the command line every unit starts and what `public-addr` logs, against `expected.txt`
  (regenerate with `UPDATE=1` and review the diff). Then: no `metanode.env` without a network,
  `--config` filling in the chains and refusing a broken file, a drop-in's `Environment=` against
  `metanode.env`, `doctor`'s configuration checks, and the bandwidth reservation on dummy uplinks
  (tree and bursts, reload, a foreign root, two default-route interfaces, stop).
- **lifecycle**, per distro: `systemd-analyze verify` and `security`, the installed layout, the scope
  of the common drop-in, then users, permissions, node keys, setup.md's ports and two RPC servers,
  WebRTC, the PVF sandbox (every security check that passes on the host must pass inside the unit),
  CPU and memory protection (on the validator and its slices), crash and outage recovery, `doctor`
  (a relay RPC of another chain, the PVF workers, a taken port), `Conflicts=`, both targets, reboot,
  an invalid value failing the unit, `rotate-keys`, `status`, `--config`, re-install, upgrade,
  uninstall (with its `enabled-roles`), the refusal to install under a unit in `/etc` or over a
  package's units or launcher, and the warning next to the `polkadot` package.
- **e2e**: a westend-local relay chain with Asset Hub and People registered at genesis. Alice and Bob
  run validators and collators across four containers on different distros: collators on the local
  validator, on a remote validator's RPC with a second one as fallback, and on an embedded relay
  node. It checks finality and that both collators author, the bandwidth reservation on one machine
  (classes and bursts, the validator's packets in its class, nftables restarts and reloads),
  `doctor`, then kills a validator (its remote collators fail over and keep authoring), reboots a
  collator machine and restarts a target. Last, a fifth machine installs from `install.sh` alone,
  piped from a web server, with `NETWORK=paseo`, which downloads and verifies Paseo's chain specs.

Separate runs (e.g. one `lifecycle` per distro) can go in parallel; each cleans up only its own
containers. Containers share the host's kernel and LSMs, so kernel-dependent results (landlock, user
namespaces, AppArmor, the bandwidth reservation's kernel check) describe the host; the harness
prints them first. `gen-local-specs.sh` needs jq >= 1.7 (older versions turn the genesis balances
into floats), so the e2e runs it in the ubuntu-24.04 image. The e2e also needs `python3` on the host
(it serves the scripts to the bootstrap install) and internet access (Paseo's chain specs).

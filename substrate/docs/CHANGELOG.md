# Changelog

The format is based on [Keep a Changelog].

[Keep a Changelog]: http://keepachangelog.com/en/1.0.0/

## Unreleased


## 2.0.0-alpha.5 -> 2.0.0-alpha.6


Runtime
-------

* Unsigned Validation best practices (#5563)
* Generate Unit Tests for Benchmarks (#5527)
* Mandate weight annotation  (#5357)
* Make Staking pallet using a proper Time module. (#4662)
* Pass transaction source to validate_transaction (#5366)
* on_initialize return weight consumed and default cost to default DispatchInfo instead of zero (#5382)

Client
------

* Add new RPC method to get the chain type (#5576)
* Reuse wasmtime instances, the PR (#5567)
* Prometheus Metrics: Turn notifications_total counter into notifications_sizes histogram (#5535)
* Make verbosity level mandatory with telemetry opt (#5057)
* Additional Metrics collected and exposed via prometheus (#5414)
* Switch to new light client protocol (#5472)
* client/finality-grandpa: Instrument until-imported queue (#5438)
* Batch benchmarks together with `*` notation. (#5436)
* src/service/src/builder: Fix memory metric exposed in bytes not KiB (#5459)
* Make transactions and block announces use notifications substre… (#5360)
* Adds state_queryStorageAt (#5362)
* Offchain Phragmén BREAKING. (#4517)
* `sc_rpc::system::SystemInfo.impl_version` now returns the full version (2.0.0-alpha.2-b950f731c-x86_64-linux-gnu) instead of the short version (1.0.0) (#5271)

API
---

* Unsigned Validation best practices (#5563)
* Split the Roles in three types (#5520)
* Pass transaction source to validate_transaction (#5366)
* on_initialize return weight consumed and default cost to default DispatchInfo instead of zero (#5382)


## 2.0.0-alpha.4 -> 2.0.0-alpha.5

Runtime
-------

* pallet-evm: configurable gasometer config (#5320)
* Adds new event phase `Initialization` (#5302)

## 2.0.0-alpha.3 -> 2.0.0-alpha.4

Runtime
-------

* Move runtime upgrade to `frame-executive` (#5197)
* Split fees and tips between author and treasury independently (#5207)
* Refactor session away from needless double_maps (#5202)
* Remove `secp256k1` from WASM build (#5187)
* Introduce default-setting prime for collective (#5137)
* Adds `vested_transfer` to Vesting pallet (#5029)
* Change extrinsic_count to extrinsic_index in pallet-utility (#5044)

Client
------

* client/finality-grandpa: Add Prometheus metrics to GossipValidator (#5237)
* removes use of sc_client::Client from node-transaction-factory (#5158)
* removes use of sc_client::Client from sc_network (#5147)
* Use CLI to configure max instances cache (#5177)
* client/service/src/builder.rs: Add build_info metric (#5192)
* Remove substrate-ui.parity.io from CORS whitelist (#5142)
* removes use of sc_client::Client from sc-rpc (#5063)
* Use 128mb for db cache default (#5134)
* Drop db-cache default from 1gig to 32mb (#5128)
* Add more metrics to prometheus (#5034)

API
---

* Produce block always on updated transaction pool state (#5227)
* Add `ext_terminate` (#5234)
* Add ext_transfer call (#5169)
* ChainSpec trait (#5185)
* client/authority-discovery: Instrument code with Prometheus (#5195)
* Don't include `:code` by default in storage proofs (#5179)
* client/network-gossip: Merge GossipEngine and GossipEngineInner (#5042)
* Introduce `on_runtime_upgrade` (#5058)

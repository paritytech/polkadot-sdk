# Changelog

The format is based on [Keep a Changelog].

[Keep a Changelog]: http://keepachangelog.com/en/1.0.0/

## Unreleased

## 2.0.0-> 2.0.1

Patch release with backports to fix broken nightly builds.
Namely contains backports of

* [#7381: Make Substrate compile with latest nightly](https://github.com/paritytech/substrate/pull/7381)
* [#7238: Fix compilation with environmental on latest nightly](https://github.com/paritytech/substrate/pull/7238)
* [#7395: Make benchmarks compile with latest nightly](https://github.com/paritytech/substrate/pull/7395)
* [#7838: Fix incorrect use of syn::exports](https://github.com/paritytech/substrate/pull/7838) (partially)
* [#7854: Update to futures 0.3.9](https://github.com/paritytech/substrate/pull/7854)


## 2.0.0-rc6 -> 2.0.0 – two dot 😮

Runtime
-------

* Rename `ModuleToIndex` to `PalletRuntimeSetup` (#7148)
* Bounties (#5715)
* pallet-collective: allow customized default vote (#6984)
* add instantiable support for treasury pallet (#7058)
* frame/authority-discovery: Have authorities() return both current and next (#6788)
* add generated weight info for pallet-collective (#6789)
* Support Staking Payout to Any Account (#6832)
* Time-delay proxies (#6770)
* Refcounts are now u32 (#7164)

Client
------

* Rename `inspect-key` to `inspect` (#7160)
* Send import notification always for re-orgs (#7118)
* Allow remotes to not open a legacy substream (#7075)
* Fix `storage::read` (#7084)
* Support hex encoded secret key for `--node-key` (#7052)
* Update the service tasks Grafana dashboard (#7038)
* manual seal is now consensus agnostic (#7010)
* Move subcommands from sc-cli to nodes (#6948)
* Implement request-responses protocols (#6634)
* fix bench db wipe (#6965)
* Fix benchmark read/write key tracker for keys in child storages. (#6905)
* *: Update to next libp2p version 0.24.0 (#6891)

API
---

* grandpa-rpc: use FinalityProofProvider to check finality for rpc (#6215)
* pow: replace the thread-base mining loop with a future-based mining worker (#7060)
* Tracing for wasm with bridging to native (#6916)
* Frame-support storage: make iterations and translate consistent (#5470)
* pow: support uniform tie breaking in fork choice (#7073)
* Make decoding of `compact<perthing>` saturating instead of invalid (#7062)
* Set reserved nodes with offchain worker. (#6996)
* client/*: Treat protocol name as str and not [u8] (#6967)
* Add a `LightSyncState` field to the chain spec (#6894)
* *: Update to next libp2p version 0.24.0 (#6891)

Runtime Migrations
------------------

* Time-delay proxies (#6770)


## 2.0.0-rc5 -> 2.0.0-rc6 – Rock Hyrax

Runtime
-------

* Custom Codec Implenetation for NPoS Election (#6720)
* Successful `note_imminent_preimage` is free (#6793)
* pallet-democracy use of weightinfo (#6783)
* Update Balances Pallet to use `WeightInfo` (#6610)
* pallet-evm: add builtin support for the four basic Ethereum precompiles (#6743)
* Allow `PostDispatchInfo` to disable fees (#6749)
* pallet-evm: add support for tuple-based precompile declarations (#6681)
* grandpa: allow noting that the set has stalled (#6725)

Client
------

* Merge Subkey into sc-cli (#4954)
* RpcHandlers Refactorings (#6846)
* client/authority-discovery: Introduce AuthorityDiscoveryService (#6760)
* Implement tracing::Event handling & parent_id for spans and events (#6672)
* Move to upstream wasmtime, refactor globals snapshot  (#6759)
* Revalidate transactions only on latest best block (#6824)
* Allow task manager to have children (#6771)
* client/network: Expose DHT query duration to Prometheus (#6784)
* client/network: Add peers to DHT only if protocols match (#6549)
* Name all the tasks! (#6726)
* Child nodes can be handled by adding a child `TaskManager` to the parent's `TaskManager` (#6771)

API
---

* pow: add access to pre-digest for algorithm verifiers (#6900)
* babe, aura, pow: only call check_inherents if authoring version is compatible (#6862)
* Implement 'transactional' annotation for runtime functions. (#6763)
* seal: Change prefix and module name from "ext_" to "seal_" for contract callable functions (#6798)
* Add Subscription RPC for Grandpa Finality (#5732)
* seal: Fix and improve error reporting (#6773)
* Allow blacklisting blocks from being finalized again after block revert (#6301)
* BABE slot and epoch event notifications (#6563)
* Add `memory-tracker` feature to `sp-trie` to fix wasm panic (#6745)

## 2.0.0-rc4 -> 2.0.0-rc5 – River Dolphin

Runtime
-------

* Support using system storage directly for EVM balance and nonce (#6659)
* Properly filter out duplicate voters in elections. (#6693)
* Treasury burning can be directed (#6671)
* identity: Don't let subs be re-registered (#6667)
* Regression test to ensure we don't break deterministic builds in wasm (#6597)
* allow to specify schedule time as a relative value (#6578)
* Make signature batching use specialized methods (#6616)
* Rename `CheckEra` to `CheckMortality` (#6619)
* Add `WeightInfo` to all pallets with benchmarks. (#6575)
* Don't require module name in inherents (#6576)
* pallet-evm: return Ok(()) when EVM execution fails (#6493)
* Make the encoded-Call Vec<u8> explicitly so in metadata (#6566)
* Allow specify schedule dispatch origin (#6387)
* pallet-evm: customizable chain id (#6537)
* Refactor as_sub to make things clearer. (#6503)

Client
------

* Update wasmtime to (almost) lastest master (#6662)
* Update to latest sysinfo prevents leaking fd-handlers (#6708)
* Tracing values (#6679)
* Graceful shutdown for the task manager (#6654)
* Update substrate-networking Grafana dashboard (#6649)
* *: Update to libp2p v0.21.1 (#6559)
* Send Status message on all newly-opened legacy substreams (#6593)
* babe: report equivocations (#6362)
* Support synching of blocks that are not `new_best` (#6508)
* Remove the service, replacing it with a struct of individual chain components (#6352)
* Fix tx-pool returning the same transaction multiple times (#6535)

API
---

* Better handling of stable-only build (#6569)
* Remove the service builder (#6557)
* seal: Prevent contracts from going below subsistence (#6623)
* seal: Rework contracts API (#6573)
* Make evm errors public (#6598)
* Add log rotation (#6564)
* decl_module! macro: use 'frame_system' instead of `system` as default ident (#6500)
* Restrict `Protected` to some heap types. (#6471)

## 2.0.0-rc3 -> 2.0.0-rc4 (Rhinoceros)

Runtime
-------

* Staking Payout Creates Controller (#6496)
* `pallet-scheduler`: Check that `when` is not in the past (#6480)
* Fix `sp-api` handling of multiple arguments (#6484)
* Fix issues with `Operational` transactions validity and prioritization. (#6435)
* pallet-atomic-swap: generialized swap action (#6421)
* Avoid multisig reentrancy (#6445)
* Root origin use no filter by default. Scheduler and Democracy dispatch without asserting BaseCallFilter (#6408)
* Scale and increase validator count (#6417)
* Pallet: Atomic Swap (#6349)
* Restrict remove_proxies (#6383)
* Stored call in multisig (#6319)
* Allow Sudo to do anything (#6375)
* vesting: Force Vested Transfer (#6368)
* Add events for balance reserve and unreserve functions (#6330)
* Introduce frozen indices. (#6307)

Client
------

* client/network/service: Add primary dimension to connection metrics (#6472)
* Fix Babe secondary plain slots claiming (#6451)
* add network propagated metrics (#6438)
* client/authority-discovery: Compare PeerIds and not Multihashes (#6414)
* Update sync chain info on own block import (#6424)
* Remove --legacy-network-protocol CLI flag (#6411)
* Runtime interface to add support for tracing from wasm (#6381)
* Remove penalty on duplicate Status message (#6377)
* Fix the broken weight multiplier update function (#6334)
* client/authority-discovery: Don't add own address to priority group (#6370)
* Split the service initialisation up into seperate functions (#6332)
* Fix transaction pool event sending (#6341)
* Add a [prefix]_process_start_time_seconds metric (#6315)
* new crate sc-light (#6235)
* Allow adding a prefix to the informant (#6174)

API
---

* seal: Remove ext_dispatch_call and ext_get_runtime_storage (#6464)
* seal: Refactor ext_gas_price (#6478)
* Implement nested storage transactions (#6269)
* Allow empty values in the storage (#6364)
* add system_dryRun (#6300)
* Introduce in-origin filtering (#6318)
* add extend_lock for StorageLock (#6323)
* Deprecate FunctionOf and remove its users (#6340)
* transaction-pool: expose blocking api for tx submission (#6325)


## 2.0.0-rc2 -> 2.0.0-rc3

Runtime
-------

* Introduce stacked filtering (#6273)
* Allow "anonymous" proxied accounts (#6236)
* Allow over-weight collective proposals to be closed (#6163)
* Fix Election when ForceNone V1 (#6166)

Client
------

* Make transaction pool prune transactions only of canonical blocks (#6123)
* Rename all the election operations (#6245)
* Sentry nodes and validator nodes also imply reserved (#6251)
* Fix peerset not filtering incoming connections in reserved-only (#6249)
* Use Subscription Manager from `jsonrpc-pubsub` (#6208)
* Add a Substrate networking Grafana dashboard template (#6171)
* Add subkey inspect-node-key (#6153)

## 2.0.0-rc1 -> 2.0.0-rc2

(nothing of note)

## 2.0.0-alpha.8 -> 2.0.0-rc1

Runtime
-------

* Allow operational recovery path if on_initialize use fullblock. (#6089)
* Maximum extrinsic weight limit (#6067)

Client
------

* Add JSON format to import blocks and set it as default (#5816)
* Upgrade to libp2p v0.19 - Changes the default PeerId representation (#6064)


## 2.0.0-alpha.7 -> 2.0.0-alpha.8

**License Changed**
From this release forward, the code is released under a new – more relaxed – license scheme: Client (`sc-*`) is released under "GPL 3.0 or newer with the Classpath Exception", while primitives, FRAME, the pallets, utils and test-utils are released under "Apache 2.0". More details in the [Relax licensing scheme PR](https://github.com/paritytech/substrate/pull/5947).

Runtime
-------

* Democracy weight (#5828)
* Make `Digest` support `StorageAppend` (#5922)

Client
------

* Meter block import results via prometheus (#6025)
* Added RuntimePublic for ecdsa public key. (#6029)
* Benchmarks for elections-phragmen pallet (#5845)
* Monitor transactions rejected from the pool as invalid (#5992)
* client/network: Remove default Kademlia DHT in favor of per protocol DHT (#5993)
* Allow passing multiple --log CLI options (#5982)
* client: Replace `unsafe_rpc_expose` with an `RpcMethods` enum (#5729)

## 2.0.0-alpha.6 -> 2.0.0-alpha.7

Runtime
-------

* Use `storage::append` in the implementation of the storage types (#5889)
* pallet-sudo: Store `DispatchResult` in `Sudid` event (#5804)
* Enable Offchain Equalise (#5683)
* Add support for custom runtime upgrade logic (#5782)
* Require `fn` token in `decl_storage` `get` (#5717)
* Child trie api changes BREAKING (#4857)
* Pass max-total to RewardRemainder on end_era (#5697)
* Transaction versioning in the RuntimeVersion (#5582)
* emit TipClosed event on success tip payout (#5656)

Client
------

* Adds `export-state` subcommand (#5842)
* Drop ClientProvider (#5823)
* Move spawning tasks from thread pools to Service's TaskManager for block importing (#5647)
* Reputation penalty for sending empty block response (#5814)
* Move sc-client into sc-service (#5502)
* Use new block requests protocol (#5760)
* Fix leak in stream notifications (#5739)
* network: Only insert global addresses into the DHT. (#5735)
* enum Pays for PaysFee (#5733)
* Migrate away from `SimpleDispatchInfo` (#5686)
* Child trie api changes BREAKING (#4857)
* subkey: compute and inspect a moduleid (#5676)
* Listen on ipv6 by default as well (#5677)
* Adjustments to Kademlia-related metrics (#5660)
* client/authority-discovery: Allow to be run by sentry node (#5568)
* Add alternative RPC methods to system_networkState (#5643)
* Several tweaks to networking Prometheus metrics (#5636)
* Use a Kademlia instance per `ProtocolId`. (#5045)
* Report tasks metrics to Prometheus (#5619)

API
---

* Child trie api changes BREAKING (#4857)
* Pass max-total to RewardRemainder on end_era (#5697)
* Implement iter for doublemap (#5504)

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

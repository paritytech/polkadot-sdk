# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.3] - 2025-12-16

This release improves the robustness of the multistream-select negotiation over WebRTC transport and fixes inbound bandwidth metering on substreams. It also enhances the dialing success rate by improving the transport dialing logic. Additionally, it re-exports CID's multihash to facilitate the construction of CID V1.

### Changed

- transports: Improves the robustness and success rate of connection dialing  ([#495](https://github.com/paritytech/litep2p/pull/495))
- types: Re-export cid's multihash to construct CID V1  ([#491](https://github.com/paritytech/litep2p/pull/491))

### Fixed

- fix: multistream-select negotiation on outbound substream over webrtc  ([#465](https://github.com/paritytech/litep2p/pull/465))
- substream: Fix inbound bandwidth metering  ([#499](https://github.com/paritytech/litep2p/pull/499))

## [0.12.2] - 2025-11-28

This release allows all Bitswap CIDs (v1) to pass regardless of the used hash. It also enhances local address checks in the transport manager.

### Changed

- transport-service: Enhance logging with protocol names  ([#485](https://github.com/paritytech/litep2p/pull/485))
- bitswap: Reexports for CID  ([#486](https://github.com/paritytech/litep2p/pull/486))
- Allow all the Bitswap CIDs (v1) to pass regardless of used hash  ([#482](https://github.com/paritytech/litep2p/pull/482))
- transport/manager: Enhance local address checks  ([#480](https://github.com/paritytech/litep2p/pull/480))

## [0.12.1] - 2025-11-21

This release adds support for connecting to multiple Kademlia DHT networks. The change is backward-compatible, no client code modifications should be needed compared to v0.12.0.

### Changed

- kad: Allow connecting to more than one DHT network ([#473](https://github.com/paritytech/litep2p/pull/473))
- service: Log services that have closed ([#474](https://github.com/paritytech/litep2p/pull/474))

### Fixed

- update simple-dns ([#470](https://github.com/paritytech/litep2p/pull/470))

## [0.12.0] - 2025-11-11

This release adds `KademliaEvent::PutRecordSuccess` & `KademliaEvent::AddProviderSuccess` events to Kademlia, allowing to track whether publishing a record or a provider was successfull. While `PutRecordSuccess` was present in the previous versions of litep2p, it was actually never emitted. Note that `AddProviderSuccess` and `QueryFailed` are also generated during automatic provider refresh, so those may be emitted for `QueryId`s not known to the client code.

### Added

- kademlia: Track success of `ADD_PROVIDER` queries ([#432](https://github.com/paritytech/litep2p/pull/432))
- kademlia: Workaround for dealing with not implemented `PUT_VALUE` ACKs ([#430](https://github.com/paritytech/litep2p/pull/430))
- kademlia: Track success of `PUT_VALUE` queries ([#427](https://github.com/paritytech/litep2p/pull/427))

### Fixed

- Identify: gracefully close substream after sending payload ([#466](https://github.com/paritytech/litep2p/pull/466))
- fix: transport context polling order ([#456](https://github.com/paritytech/litep2p/pull/456))

### Changed

- refactor: implement builder pattern for TransportManager ([#453](https://github.com/paritytech/litep2p/pull/453))

## [0.11.1] - 2025-10-28

This release ensures that polling the yamux controller after an error does not lead to unexpected behavior.

### Fixed

- yamux/control: Ensure poll next inbound is not called after errors  ([#445](https://github.com/paritytech/litep2p/pull/445))

## [0.11.0] - 2025-10-20

This release adds support for RSA remote network identity keys gated behind `rsa` feature. It also fixes mDNS initialization in the environment with no multicast addresses available and Bitswap compatibility with kubo IPFS client >= v0.37.0.

### Added

- Support RSA remote network identity keys ([#423](https://github.com/paritytech/litep2p/pull/423))

### Fixed

- bitswap: Reuse inbound substream for subsequent requests ([#447](https://github.com/paritytech/litep2p/pull/447))
- mDNS: Do not fail initialization if the socket could not be created ([#434](https://github.com/paritytech/litep2p/pull/434))
- Make RemotePublicKey public to enable signature verification ([#435](https://github.com/paritytech/litep2p/pull/435))
- improve error handling in webRTC-related noise function ([#377](https://github.com/paritytech/litep2p/pull/377))

### Changed

- Upgrade rcgen 0.10.0 -> 0.14.5 ([#450](https://github.com/paritytech/litep2p/pull/450))
- chore: update str0m dependency, update code based on breaking changes ([#422](https://github.com/paritytech/litep2p/pull/422))

## [0.10.0] - 2025-07-22

This release adds the ability to use system DNS resolver and change Kademlia DNS memory store capacity. It also fixes the Bitswap protocol implementation and correctly handles the dropped notification substreams by unregistering them from the protocol list.

### Added

- kad: Expose memory store configuration ([#407](https://github.com/paritytech/litep2p/pull/407))
- transport: Allow changing DNS resolver config ([#384](https://github.com/paritytech/litep2p/pull/384))

### Fixed

- notification: Unregister dropped protocols ([#391](https://github.com/paritytech/litep2p/pull/391))
- bitswap: Fix protocol implementation ([#402](https://github.com/paritytech/litep2p/pull/402))
- transport-manager: stricter supported multiaddress check ([#403](https://github.com/paritytech/litep2p/pull/403))

## [0.9.5] - 2025-05-26

This release primarily focuses on strengthening the stability of the websocket transport. We've resolved an issue where higher-level buffering was causing the Noise protocol to fail when decoding messages.

We've also significantly improved connectivity between litep2p and Smoldot (the Substrate-based light client). Empty frames are now handled correctly, preventing handshake timeouts and ensuring smoother communication.

Finally, we've carried out several dependency updates to keep the library current with the latest versions of its underlying components.

### Fixed

- substream/fix: Allow empty payloads with 0-length frame  ([#395](https://github.com/paritytech/litep2p/pull/395))
- websocket: Fix connection stability on decrypt messages  ([#393](https://github.com/paritytech/litep2p/pull/393))

### Changed

- crypto/noise: Show peerIDs that fail to decode  ([#392](https://github.com/paritytech/litep2p/pull/392))
- cargo: Bump yamux to 0.13.5 and tokio to 1.45.0  ([#396](https://github.com/paritytech/litep2p/pull/396))
- ci: Enforce and apply clippy rules  ([#388](https://github.com/paritytech/litep2p/pull/388))
- build(deps): bump ring from 0.16.20 to 0.17.14  ([#389](https://github.com/paritytech/litep2p/pull/389))
- Update hickory-resolver 0.24.2 -> 0.25.2  ([#386](https://github.com/paritytech/litep2p/pull/386))

## [0.9.4] - 2025-05-01

This release brings several improvements and fixes to litep2p, advancing its stability and readiness for production use.

### Performance Improvements

This release addresses an issue where notification protocols failed to exit on handle drop, lowering CPU usage in scenarios like minimal-relay-chains from 7% to 0.1%.

### Robustness Improvements

- Kademlia:
  - Optimized address store by sorting addresses based on dialing score, bounding memory consumption and improving efficiency.
  - Limited `FIND_NODE` responses to the replication factor, reducing data stored in the routing table.
  - Address store improvements enhance robustness against routing table alterations.

- Identify Codec:
  - Enhanced message decoding to manage malformed or unexpected messages gracefully.

- Bitswap:
  - Introduced a write timeout for sending frames, preventing protocol hangs or delays.

### Testing and Reliability

- Fuzzing Harness: Added a fuzzing harness by SRLabs to uncover and resolve potential issues, improving code robustness. Thanks to @R9295 for the contribution!

- Testing Enhancements: Improved notification state machine testing. Thanks to Dominique (@Imod7) for the contribution!

### Dependency Management

- Updated all dependencies for stable feature flags (default and "websocket") to their latest versions.

- Reorganized dependencies under specific feature flags, shrinking the default feature set and avoiding exposure of outdated dependencies from experimental features.

### Fixed

- notifications: Exit protocols on handle drop to save up CPU of `minimal-relay-chains`  ([#376](https://github.com/paritytech/litep2p/pull/376))
- identify: Improve identify message decoding  ([#379](https://github.com/paritytech/litep2p/pull/379))
- crypto/noise: Set timeout limits for the noise handshake  ([#373](https://github.com/paritytech/litep2p/pull/373))
- kad: Improve robustness of addresses from the routing table  ([#369](https://github.com/paritytech/litep2p/pull/369))
- kad: Bound kademlia messages to the replication factor  ([#371](https://github.com/paritytech/litep2p/pull/371))
- codec: Decode smaller payloads for identity to None  ([#362](https://github.com/paritytech/litep2p/pull/362))

### Added

- bitswap: Add write timeout for sending frames  ([#361](https://github.com/paritytech/litep2p/pull/361))
- notif/tests: check test state  ([#360](https://github.com/paritytech/litep2p/pull/360))
- SRLabs: Introduce simple fuzzing harness  ([#367](https://github.com/paritytech/litep2p/pull/367))
- SRLabs: Introduce Fuzzing Harness  ([#365](https://github.com/paritytech/litep2p/pull/365))

### Changed

- features: Move quic related dependencies under feature flag  ([#359](https://github.com/paritytech/litep2p/pull/359))
- tests/substrate: Remove outdated substrate specific conformace testing  ([#370](https://github.com/paritytech/litep2p/pull/370))
- ci: Update stable dependencies  ([#375](https://github.com/paritytech/litep2p/pull/375))
- build(deps): bump hex-literal from 0.4.1 to 1.0.0  ([#381](https://github.com/paritytech/litep2p/pull/381))
- build(deps): bump tokio from 1.44.1 to 1.44.2 in /fuzz/structure-aware  ([#378](https://github.com/paritytech/litep2p/pull/378))
- build(deps): bump Swatinem/rust-cache from 2.7.7 to 2.7.8  ([#363](https://github.com/paritytech/litep2p/pull/363))
- build(deps): bump tokio from 1.43.0 to 1.43.1  ([#368](https://github.com/paritytech/litep2p/pull/368))
- build(deps): bump openssl from 0.10.70 to 0.10.72  ([#366](https://github.com/paritytech/litep2p/pull/366))

## [0.9.3] - 2025-03-11

This release introduces an API for setting the maximum kademlia message size.

### Changed

- kad: Set upper limits for kademlia messages  ([#357](https://github.com/paritytech/litep2p/pull/357))

## [0.9.2] - 2025-03-10

This release downgrades a spamming log message to debug level and adds tests for the WebSocket stream implementation.

Thanks to @dharjeezy for contributing to this release by avoiding to clone the Kademlia peers when yielding the closest nodes!

### Changed

- manager: Downgrade refusing to add address log to debug  ([#355](https://github.com/paritytech/litep2p/pull/355))
- Clone only needed KademliaPeers when yielding closest nodes  ([#326](https://github.com/paritytech/litep2p/pull/326))

### Added

- websocket/stream/tests: Add tests for the stream implementation  ([#329](https://github.com/paritytech/litep2p/pull/329))

## [0.9.1] - 2025-01-19

This release enhances compatibility between litep2p and libp2p by using the latest Yamux upstream version. Additionally, it includes various improvements and fixes to boost the stability and performance of the WebSocket stream and the multistream-select protocol.

### Changed

- yamux: Switch to upstream implementation while keeping the controller API  ([#320](https://github.com/paritytech/litep2p/pull/320))
- req-resp: Replace SubstreamSet with FuturesStream  ([#321](https://github.com/paritytech/litep2p/pull/321))
- cargo: Bring up to date multiple dependencies  ([#324](https://github.com/paritytech/litep2p/pull/324))
- build(deps): bump hickory-proto from 0.24.1 to 0.24.3  ([#323](https://github.com/paritytech/litep2p/pull/323))
- build(deps): bump openssl from 0.10.66 to 0.10.70  ([#322](https://github.com/paritytech/litep2p/pull/322))

### Fixed

- websocket/stream: Fix unexpected EOF on `Poll::Pending` state poisoning  ([#327](https://github.com/paritytech/litep2p/pull/327))
- websocket/stream: Avoid memory allocations on flushing  ([#325](https://github.com/paritytech/litep2p/pull/325))
- multistream-select: Enforce `io::error` instead of empty protocols  ([#318](https://github.com/paritytech/litep2p/pull/318))
- multistream: Do not wait for negotiation in poll_close  ([#319](https://github.com/paritytech/litep2p/pull/319))


## [0.9.0] - 2025-01-14

This release provides partial results to speed up `GetRecord` queries in the Kademlia protocol.

### Changed

- kad: Provide partial results to speedup `GetRecord` queries  ([#315](https://github.com/paritytech/litep2p/pull/315))

## [0.8.4] - 2024-12-12

This release aims to make the MDNS component more robust by fixing a bug that caused the MDNS service to fail to register opened substreams. Additionally, the release includes several improvements to the `identify` protocol, replacing `FuturesUnordered` with `FuturesStream` for better performance.

### Fixed

- mdns/fix: Failed to register opened substream  ([#301](https://github.com/paritytech/litep2p/pull/301))

### Changed

- identify: Replace FuturesUnordered with FuturesStream  ([#302](https://github.com/paritytech/litep2p/pull/302))
- chore: Update hickory-resolver to version 0.24.2  ([#304](https://github.com/paritytech/litep2p/pull/304))
- ci: Ensure cargo-machete is working with rust version from CI  ([#303](https://github.com/paritytech/litep2p/pull/303))

## [0.8.3] - 2024-12-03

This release includes two fixes for small memory leaks happening on edge-cases in the notification and request-response protocols.

### Fixed

- req-resp: Fix memory leak of pending substreams  ([#297](https://github.com/paritytech/litep2p/pull/297))
- notification: Fix memory leak of pending substreams ([#296](https://github.com/paritytech/litep2p/pull/296))

## [0.8.2] - 2024-11-27

This release ensures that the provided peer identity is verified at the crypto/noise protocol level, enhancing security and preventing potential misuses.
The release also includes a fix that caused `TransportService` component to panic on debug builds.

### Fixed

- req-resp: Fix panic on connection closed for substream open failure  ([#291](https://github.com/paritytech/litep2p/pull/291))
- crypto/noise: Verify crypto/noise signature payload  ([#278](https://github.com/paritytech/litep2p/pull/278))

### Changed

- transport_service/logs: Provide less details for trace logs  ([#292](https://github.com/paritytech/litep2p/pull/292))

## [0.8.1] - 2024-11-14

This release includes key fixes that enhance the stability and performance of the litep2p library, focusing on long-running stability and improvements to polling mechanisms.

### Long Running Stability Improvements

This issue caused long-running nodes to reject all incoming connections, impacting overall stability.

Addressed a bug in the connection limits functionality that incorrectly tracked connections due for rejection.
This issue caused an artificial increase in inbound peers, which were not being properly removed from the connection limit count.
This fix ensures more accurate tracking and management of peer connections [#286](https://github.com/paritytech/litep2p/pull/286).

### Polling implementation fixes

This release provides multiple fixes to the polling mechanism, improving how connections and events are processed:

- Resolved an overflow issue in TransportContext's polling index for streams, preventing potential crashes.
- Fixed a delay in the manager's `poll_next` function that prevented immediate polling of newly added futures.
- Corrected an issue where the listener did not return Poll::Ready(None) when it was closed, ensuring proper signal handling.

### Fixed

- manager: Fix connection limits tracking of rejected connections  ([#286](https://github.com/paritytech/litep2p/pull/286))
- transport: Fix waking up on filtered events from `poll_next`  ([#287](https://github.com/paritytech/litep2p/pull/287))
- transports: Fix missing Poll::Ready(None) event from listenener  ([#285](https://github.com/paritytech/litep2p/pull/285))
- manager: Avoid overflow on stream implementation for `TransportContext`  ([#283](https://github.com/paritytech/litep2p/pull/283))
- manager: Log when polling returns Ready(None)  ([#284](https://github.com/paritytech/litep2p/pull/284))

## [0.8.0] - 2024-11-04

This release adds support for content provider advertisement and discovery to Kademlia protocol implementation (see libp2p [spec](https://github.com/libp2p/specs/blob/master/kad-dht/README.md#content-provider-advertisement-and-discovery)).
Additionally, the release includes several improvements and memory leak fixes to enhance the stability and performance of the litep2p library.

### [Content Provider Advertisement and Discovery](https://github.com/paritytech/litep2p/pull/234)

Litep2p now supports content provider advertisement and discovery through the Kademlia protocol.
Content providers can publish their records to the network, and other nodes can discover and retrieve these records using the `GET_PROVIDERS` query.

```rust
    // Start providing a record to the network.
    // This stores the record in the local provider store and starts advertising it to the network.
    kad_handle.start_providing(key.clone());

    // Wait for some condition to stop providing...

    // Stop providing a record to the network.
    // The record is removed from the local provider store and stops advertising it to the network.
    // Please note that the record will be removed from the network after the TTL expires.
    kad_provider.stop_providing(key.clone());

    // Retrieve providers for a record from the network.
    // This returns a query ID that is later producing the result when polling the `Kademlia` instance.
    let query_id = kad_provider.get_providers(key.clone());
```

### Added

- kad: Providers part 8: unit, e2e, and `libp2p` conformance tests  ([#258](https://github.com/paritytech/litep2p/pull/258))
- kad: Providers part 7: better types and public API, public addresses & known providers  ([#246](https://github.com/paritytech/litep2p/pull/246))
- kad: Providers part 6: stop providing  ([#245](https://github.com/paritytech/litep2p/pull/245))
- kad: Providers part 5: `GET_PROVIDERS` query  ([#236](https://github.com/paritytech/litep2p/pull/236))
- kad: Providers part 4: refresh local providers  ([#235](https://github.com/paritytech/litep2p/pull/235))
- kad: Providers part 3: publish provider records (start providing)  ([#234](https://github.com/paritytech/litep2p/pull/234))

### Changed

- transport_service: Improve connection stability by downgrading connections on substream inactivity  ([#260](https://github.com/paritytech/litep2p/pull/260))
- transport: Abort canceled dial attempts for TCP, WebSocket and Quic  ([#255](https://github.com/paritytech/litep2p/pull/255))
- kad/executor: Add timeout for writting frames  ([#277](https://github.com/paritytech/litep2p/pull/277))
- kad: Avoid cloning the `KademliaMessage` and use reference for `RoutingTable::closest`  ([#233](https://github.com/paritytech/litep2p/pull/233))
- peer_state: Robust state machine transitions  ([#251](https://github.com/paritytech/litep2p/pull/251))
- address_store: Improve address tracking and add eviction algorithm  ([#250](https://github.com/paritytech/litep2p/pull/250))
- kad: Remove unused serde cfg  ([#262](https://github.com/paritytech/litep2p/pull/262))
- req-resp: Refactor to move functionality to dedicated methods  ([#244](https://github.com/paritytech/litep2p/pull/244))
- transport_service: Improve logs and move code from tokio::select macro  ([#254](https://github.com/paritytech/litep2p/pull/254))

### Fixed

- tcp/websocket/quic: Fix cancel memory leak  ([#272](https://github.com/paritytech/litep2p/pull/272))
- transport: Fix pending dials memory leak  ([#271](https://github.com/paritytech/litep2p/pull/271))
- ping: Fix memory leak of unremoved `pending_opens`  ([#274](https://github.com/paritytech/litep2p/pull/274))
- identify: Fix memory leak of unused `pending_opens`  ([#273](https://github.com/paritytech/litep2p/pull/273))
- kad: Fix not retrieving local records  ([#221](https://github.com/paritytech/litep2p/pull/221))

## [0.7.0] - 2024-09-05

This release introduces several new features, improvements, and fixes to the litep2p library. Key updates include enhanced error handling, configurable connection limits, and a new API for managing public addresses.

### [Exposing Public Addresses API](https://github.com/paritytech/litep2p/pull/212)

A new `PublicAddresses` API has been added, enabling users to manage the node's public addresses. This API allows developers to add, remove, and retrieve public addresses, which are shared with peers through the Identify protocol.

```rust
    // Public addresses are accessible from the main litep2p instance.
    let public_addresses = litep2p.public_addresses();

    // Add a new public address to the node.
    if let Err(err) = public_addresses.add_address(multiaddr) {
        eprintln!("Failed to add public address: {:?}", err);
    }

    // Remove a public address from the node.
    public_addresses.remove_address(&multiaddr);

    // Retrieve all public addresses of the node.
    for address in public_addresses.get_addresses() {
        println!("Public address: {}", address);
    }
```

**Breaking Change**: The Identify protocol no longer includes public addresses in its configuration. Instead, use the new `PublicAddresses` API.

Migration Guide:

```rust
    // Before:
    let (identify_config, identify_event_stream) = IdentifyConfig::new(
        "/substrate/1.0".to_string(),
        Some(user_agent),
        config.public_addresses,
    );

    // After:
    let (identify_config, identify_event_stream) =
        IdentifyConfig::new("/substrate/1.0".to_string(), Some(user_agent));
    // Public addresses must now be added using the `PublicAddresses` API:
    for address in config.public_addresses {
        if let Err(err) = public_addresses.add_address(address) {
            eprintln!("Failed to add public address: {:?}", err);
        }
    }
```

### [Dial Error and List Dial Failures Event](https://github.com/paritytech/litep2p/pull/206)

The `DialFailure` event has been enhanced with a new `DialError` enum for more precise error reporting when a dial attempt fails. Additionally, a `ListDialFailures` event has been introduced, listing all dialed addresses and their corresponding errors when multiple addresses are involved.

Other litep2p errors, such as `ParseError`, `AddressError`, and `NegotiationError`, have been refactored for improved error propagation.

### [Immediate Dial Error and Request-Response Rejection Reasons](https://github.com/paritytech/litep2p/pull/227)

This new feature paves the way for better error handling in the `litep2p` library and moves away from the overarching `litep2p::error::Error` enum.

The newly added `ImmediateDialError` enum captures errors occurring before a dial request is sent (e.g., missing peer IDs). It also enhances the `RejectReason` enum for request-response protocols, offering more detailed rejection reasons.


```rust
match error {
    RequestResponseError::Rejected(reason) => {
        match reason {
            RejectReason::ConnectionClosed => "connection-closed",
            RejectReason::DialFailed(Some(ImmediateDialError::AlreadyConnected)) => "already-connected",
            _ => "other",
        }
    }
    _ => "other",
}
```

### [Connection Limits](https://github.com/paritytech/litep2p/pull/185)

Developers can now set limits on the number of inbound and outbound connections to manage resources and optimize performance.

```rust
    // Configure connection limits for inbound and outbound established connections.
    let litep2p_config = Config::default()
        .with_connection_limits(ConnectionLimitsConfig::default()
            .max_incoming_connections(Some(3))
            .max_outgoing_connections(Some(2))
        );
```

### [Feature Flags for Optional Transports](https://github.com/paritytech/litep2p/pull/192)

The library now supports feature flags to selectively enable or disable transport protocols. By default, only the `TCP` transport is enabled. Optional transports include:

- `quic` - Enables QUIC transport.
- `websocket` - Enables WebSocket transport.
- `webrtc` - Enables WebRTC transport.

### [Configurable Keep-Alive Timeout](https://github.com/paritytech/litep2p/pull/155)

The keep-alive timeout for connections is now configurable, providing more control over connection lifecycles.

```rust
    // Set keep alive timeout for connections.
    let litep2p_config = Config::default()
        .with_keep_alive_timeout(Duration::from_secs(30));
```

Thanks for contributing to this @[Ma233](https://github.com/Ma233)!

### Added

- errors: Introduce immediate dial error and request-response rejection reasons  ([#227](https://github.com/paritytech/litep2p/pull/227))
- Expose API for `PublicAddresses`  ([#212](https://github.com/paritytech/litep2p/pull/212))
- transport: Implement `TransportService::local_peer_id()`  ([#224](https://github.com/paritytech/litep2p/pull/224))
- find_node: Optimize parallelism factor for slow to respond peers  ([#220](https://github.com/paritytech/litep2p/pull/220))
- kad: Handle `ADD_PROVIDER` & `GET_PROVIDERS` network requests  ([#213](https://github.com/paritytech/litep2p/pull/213))
- errors: Add `DialError` error and `ListDialFailures` event for better error reporting  ([#206](https://github.com/paritytech/litep2p/pull/206))
- kad: Add support for provider records to `MemoryStore`  ([#200](https://github.com/paritytech/litep2p/pull/200))
- transport: Add accept_pending/reject_pending for inbound connections and introduce inbound limits  ([#194](https://github.com/paritytech/litep2p/pull/194))
- transport/manager: Add connection limits for inbound and outbound established connections  ([#185](https://github.com/paritytech/litep2p/pull/185))
- kad: Add query id to log messages  ([#174](https://github.com/paritytech/litep2p/pull/174))

### Changed

- transport: Replace trust_dns_resolver with hickory_resolver  ([#223](https://github.com/paritytech/litep2p/pull/223))
- crypto/noise: Generate keypair only for Curve25519  ([#214](https://github.com/paritytech/litep2p/pull/214))
- transport: Allow manual setting of keep-alive timeout  ([#155](https://github.com/paritytech/litep2p/pull/155))
- kad: Update connection status of an existing bucket entry  ([#181](https://github.com/paritytech/litep2p/pull/181))
- Make transports optional  ([#192](https://github.com/paritytech/litep2p/pull/192))

### Fixed

- kad: Fix substream opening and dialing race  ([#222](https://github.com/paritytech/litep2p/pull/222))
- query-executor: Save the task waker on empty futures  ([#219](https://github.com/paritytech/litep2p/pull/219))
- substream: Use write_all instead of manually writing bytes  ([#217](https://github.com/paritytech/litep2p/pull/217))
- minor: fix tests without `websocket` feature  ([#215](https://github.com/paritytech/litep2p/pull/215))
- Fix TCP, WebSocket, QUIC leaking connection IDs in `reject()`  ([#198](https://github.com/paritytech/litep2p/pull/198))
- transport: Fix double lock and state overwrite on disconnected peers  ([#179](https://github.com/paritytech/litep2p/pull/179))
- kad: Do not update memory store on incoming `GetRecordSuccess`  ([#190](https://github.com/paritytech/litep2p/pull/190))
- transport: Reject secondary connections with different connection IDs  ([#176](https://github.com/paritytech/litep2p/pull/176))

## [0.6.2] - 2024-06-26

This is a bug fixing release. Kademlia now correctly sets and forwards publisher & ttl in the DHT records.

### Fixed

- kademlia: Preserve publisher & expiration time in DHT records ([#162](https://github.com/paritytech/litep2p/pull/162))

## [0.6.1] - 2024-06-20

This is a bug fixing and security release. curve255190-dalek has been upgraded to v4.1.3, see [dalek-cryptography/curve25519-dalek#659](https://github.com/dalek-cryptography/curve25519-dalek/pull/659) for details.

### Fixed

- kad: Set default ttl 36h for kad records ([#154](https://github.com/paritytech/litep2p/pull/154))
- chore: update ed25519-dalek to v2.1.1 ([#122](https://github.com/paritytech/litep2p/pull/122))
- Bump curve255190-dalek 4.1.2 -> 4.1.3 ([#159](https://github.com/paritytech/litep2p/pull/159))

## [0.6.0] - 2024-06-14

This release introduces breaking changes into `kad` module. The API has been extended as following:

- An event `KademliaEvent::IncomingRecord` has been added.
- New methods `KademliaHandle::store_record()` / `KademliaHandle::try_store_record()` have been introduced.

This allows implementing manual incoming DHT record validation by configuring `Kademlia` with `IncomingRecordValidationMode::Manual`.

Also, it is now possible to enable `TCP_NODELAY` on sockets.

Multiple refactorings to remove the code duplications and improve the implementation robustness have been done.

### Added

- Support manual DHT record insertion ([#135](https://github.com/paritytech/litep2p/pull/135))
- transport: Make `TCP_NODELAY` configurable ([#146](https://github.com/paritytech/litep2p/pull/146))

### Changed

- transport: Introduce common listener for tcp and websocket ([#147](https://github.com/paritytech/litep2p/pull/147))
- transport/common: Share DNS lookups between TCP and WebSocket ([#151](https://github.com/paritytech/litep2p/pull/151))

### Fixed

- ping: Make ping fault tolerant wrt outbound substreams races ([#133](https://github.com/paritytech/litep2p/pull/133))
- crypto/noise: Make noise fault tolerant ([#142](https://github.com/paritytech/litep2p/pull/142))
- protocol/notif: Fix panic on missing peer state ([#143](https://github.com/paritytech/litep2p/pull/143))
- transport: Fix erroneous handling of secondary connections ([#149](https://github.com/paritytech/litep2p/pull/149))

## [0.5.0] - 2024-05-24

This is a small release that makes the `FindNode` command a bit more robust:

- The `FindNode` command now retains the K (replication factor) best results.
- The `FindNode` command has been updated to handle errors and unexpected states without panicking.

### Added

- Add release checklist  ([#115](https://github.com/paritytech/litep2p/pull/115))

### Changed

- kad: Refactor FindNode query, keep K best results and add tests  ([#114](https://github.com/paritytech/litep2p/pull/114))

## [0.4.0] - 2024-05-23

This release introduces breaking changes to the litep2p crate, primarily affecting the `kad` module. Key updates include:

- The `GetRecord` command now exposes all peer records, not just the latest one.
- A new `RecordType` has been introduced to clearly distinguish between locally stored records and those discovered from the network.

Significant refactoring has been done to enhance the efficiency and accuracy of the `kad` module. The updates are as follows:

- The `GetRecord` command now exposes all peer records.
- The `GetRecord` command has been updated to handle errors and unexpected states without panicking.

Additionally, we've improved code coverage in the `kad` module by adding more tests.

### Added

- Re-export `multihash` & `multiaddr` types  ([#79](https://github.com/paritytech/litep2p/pull/79))
- kad: Expose all peer records of `GET_VALUE` query  ([#96](https://github.com/paritytech/litep2p/pull/96))

### Changed

- multistream_select: Remove unneeded changelog.md  ([#116](https://github.com/paritytech/litep2p/pull/116))
- kad: Refactor `GetRecord` query and add tests  ([#97](https://github.com/paritytech/litep2p/pull/97))
- kad/store: Set memory-store on an incoming record for PutRecordTo  ([#88](https://github.com/paritytech/litep2p/pull/88))
- multistream: Dialer deny multiple /multistream/1.0.0 headers  ([#61](https://github.com/paritytech/litep2p/pull/61))
- kad: Limit MemoryStore entries  ([#78](https://github.com/paritytech/litep2p/pull/78))
- Refactor WebRTC code  ([#51](https://github.com/paritytech/litep2p/pull/51))
- Revert "Bring `rustfmt.toml` in sync with polkadot-sdk (#71)"  ([#74](https://github.com/paritytech/litep2p/pull/74))
- cargo: Update str0m from 0.4.1 to 0.5.1  ([#95](https://github.com/paritytech/litep2p/pull/95))

### Fixed

- Fix clippy  ([#83](https://github.com/paritytech/litep2p/pull/83))
- crypto: Don't panic on unsupported key types  ([#84](https://github.com/paritytech/litep2p/pull/84))

## [0.3.0] - 2024-04-05

### Added

- Expose `reuse_port` option for TCP and WebSocket transports  ([#69](https://github.com/paritytech/litep2p/pull/69))
- protocol/mdns: Use `SO_REUSEPORT` for the mDNS socket  ([#68](https://github.com/paritytech/litep2p/pull/68))
- Add support for protocol/agent version  ([#64](https://github.com/paritytech/litep2p/pull/64))

## [0.2.0] - 2023-09-05

This is the second release of litep2p, v0.2.0. The quality of the first release was so bad that this release is a complete rewrite of the library.

Support is added for the following features:

* Transport protocols:
  * TCP
  * QUIC
  * WebRTC
  * WebSocket

* Protocols:
  * [`/ipfs/identify/1.0.0`](https://github.com/libp2p/specs/tree/master/identify)
  * [`/ipfs/ping/1.0.0`](https://github.com/libp2p/specs/blob/master/ping/ping.md)
  * [`/ipfs/kad/1.0.0`](https://github.com/libp2p/specs/tree/master/kad-dht)
  * [`/ipfs/bitswap/1.2.0`](https://github.com/ipfs/specs/blob/main/BITSWAP.md)
  * Request-response protocol
  * Notification protocol
  * Multicast DNS
  * API for creating custom protocols

This time the architecture has been designed to be extensible and integrating new transport and/or user-level protocols should be easier. Additionally, the test coverage is higher both in terms of unit and integration tests. The project also contains conformance tests which test the behavior of `litep2p` against, [`rust-libp2p`](https://github.com/libp2p/rust-libp2p/), [`go-libp2p`](https://github.com/libp2p/go-libp2p/) and Substrate's [`sc-network`](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/client/network). Currently the Substrate conformance tests are not enabled by default as they require unpublished/unaccepted changes to Substrate.

## [0.1.0] - 2023-04-04

This is the first release of `litep2p`, v0.1.0.

Support is added for the following:

* TCP + Noise + Yamux (compatibility with `libp2p`)
* [`/ipfs/identify/1.0.0`](https://github.com/libp2p/specs/tree/master/identify)
* [`/ipfs/ping/1.0.0`](https://github.com/libp2p/specs/blob/master/ping/ping.md)
* Request-response protocol
* Notification protocol

The code quality is atrocious but it works and the second release focuses on providing high test coverage for the library. After that is done and most of the functionality is covered (unit, integration and conformance tests, benchmarks), the focus can be turned to refactoring the code into something clean and efficient.

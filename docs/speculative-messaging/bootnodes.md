# Bootnode Discovery

Parachains need to communicate offchain using the p2p protocols to fetch messages.
On Polkadot, parachain nodes advertise themselves on the relay chain DHT ([RFC-0008](https://github.com/polkadot-fellows/RFCs/blob/main/text/0008-parachain-bootnodes-dht.md)).
Since JAM doesn't use a DHT, this document highlights the equivalent mechanism in the JAM ecosystem.

## Summary

Parachains publish a short list of bootnodes in their own key-value store in the Parachain Service, under the key `bootnodes/v1`.
Each entry in the list is a full multiaddress ending with the `/p2p` suffix.

The parachain governance updates this list through a pallet which doesn't track any storage and lets the parachain pick who may change the list.

When nodes from other parachains need to exchange offchain messages, they read this list from the finalized JAM state and use the
received entries to connect to the target parachain's nodes.

## Pallet Bootnodes

The bootnodes pallet verifies the provided multiaddresses and performs a `jam::set_kv("bootnodes/v1", encoded entries)`.

```rust
#[pallet::config]
pub trait Config: frame_system::Config + pallet_jam_kv::Config {
    /// Determines who can set the bootnode value.
    type BootnodesOrigin: EnsureOrigin<Self::RuntimeOrigin>;
}

/// The multiaddress as UTF-8 text.
/// Obtained via: `address.to_string().into_bytes()`.
type TextMultiAddr = BoundedVec<u8, ConstU32<512>>;

/// Optional fork ID of the chain.
type ForkId = Option<BoundedVec<u8, ConstU32<32>>>;

pub fn set_bootnodes(origin, addresses: BoundedVec<TextMultiAddr, ConstU32<8>>, fork_id: ForkId)
pub fn clear_bootnodes(origin)
```

The `set_bootnodes` function accepts up to 8 multiaddresses.

Each multiaddress is provided as UTF-8 text (i.e. `address.to_string().into_bytes()`).
This ensures that governance can easily provide the multiaddresses from chain-specs without needing to manually parse them.

Before writing to the key-value store, the pallet parses the received addresses and ensures it is valid:
- The multiaddress follows the standard format:`/<ip4|ip6|dns|dns4|dns6>/<host>/tcp/<port>[/ws|/wss]/p2p/<peerId>`.
- At least one address is provided.
- The port is in range 1 to 65535.
- A DNS host is a valid name, not `localhost` and not an IP address.
- The peer ID is an Ed25519 peer ID.
- The fork ID contains only letters, digits, `-` and `_`.


Once validated, the pallet writes a SCALE-encoded record to the JAM KV store:

```rust
pub struct BootnodeRecord {
    /// Genesis hash of the chain.
    genesis: [u8; 32],
    /// Fork ID of the chain.
    fork_id: ForkId,
    /// Binary representation of each multiaddress (`address.to_vec()`).
    addresses: Vec<Vec<u8>>,
}
```

The pallet has access to the genesis hash under `frame_system::Pallet::<T>::block_hash(0)`, which is never pruned.
Within the record, addresses are stored in their binary format, where each protocol consists of a code number followed by its raw value.
The format is similar to authority discovery on DHT records and is obtained by calling `address.to_vec()`.

Because setting a value in the KV store can fail with insufficient balance, the governance is responsible for monitoring the state in JAM and resubmitting the transaction at a later time if it fails.

## Consumers

A node can read its own parachain's record and use it to bootstrap its own network.
For offchain messaging with other parachains, a node subscribes to KV store changes using a JAM-specific RPC method,
tracking the keys of any parachain it needs to fetch payloads from.

By default, addresses containing non-global (private) IP addresses are ignored on production networks unless the `--allow-private-ip` flag is specified.

For every external parachain a node wants to communicate with, a client-mode `Kademlia` instance is created at runtime.
A client-mode Kademlia instance does not respond to inbound requests and is not advertised as a supported protocol.
This prevents it from polluting the target parachain's DHT.

The bootnode record contains both the genesis hash and the fork ID of the target parachain.
These values are used to generate the `/genesis/fork_id/kad` protocol name for the `Kademlia` instance.
The multiaddresses from the record are used to populate the routing table, ensuring that discovery queries use the bootnodes to find further peers on that network.

Finally, it is possible for peers to remain connected at the litep2p network backend, while being evicted from the runtime `Kademlia` routing table.
The node maintains an independent record of discovered peers that is used to issue requests for offchain messaging.

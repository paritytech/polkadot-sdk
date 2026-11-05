# Bootnode Discovery

Parachains need to communicate offchain using the p2p protocols to fetch messages.
For polkadot-sdk, the bootnodes on DHT feature keeps the closest 20 bootnodes.
For JAM, this document highlights the equivalent feature since there is no DHT.

## Summary

Parachains publish a short list with bootnodes in its onw key-value store in the Parachain Service, under the key `bootnodes/v1`.
Each value in the list represents the full multiaddress ending with the `/p2p` suffix. 

The parachain guveranance writes it through a pallet which doens't track any storage, which configures the maximum number of bootnodes, and lets the parachain pick who may change the list.

Nodes of the other parachains involved in offchain messaging read it from the finalized JAM state and use the entires to reach the parachain's nodes.

## Pallet Bootnodes

The bootnodes pallet verifies the provided muttiaddresses and performs a `jam::set_kv("bootnodes/v1", encoded entries)`.

```rust
#[pallet::config]
pub trait Config: frame_system::Config + pallet_jam_kv::Config {
    /// Determins who can set the bootnode value.
    type BootnodesOrigin: EnsureOrigin<Self::RuntimeOrigin>;
}

/// The text binary representation of a multiaddress.
/// As obtained by: `address.to_string().into_bytes()`.
type MultiAddr = BoundedVec<u8, 512>;

pub fn set_bootnodes(origin, record: BoundedVec<MultiAddr, ConstU32<8>>)
pub fn clear_bootnodes(origin)
```

The pallet receives a maximum of 8 multiaddresses as arguments in `set_bootnodes`.
The received mutliaddress is the binary represenation of the address in plain text (ie `address.to_string().into_bytes()`). This ensures that governance can paste the multiaddresses from chain-specs without parsing them further.

The pallet decodes the received multiaddress, ensures they are unique, IP addresses are global and follow the format:
```
/<ip4|ip6|dns|dns4|dns6>/<host>/tcp/<port>[/ws|/wss]/p2p/<peerId>
```

Then, the addresses are stored in the KV store in their binary format (ie `address.to_vec()` similar to authority records on DHT), where each protocol is a code number followed by its value in raw form.

The pallet needs no strage to hold these records. Since KV store set can fail with insufficient balance, the governance is responsible for monitoring the state in JAM and resubmit the transaction at a later time if it fails.

## Receivers
  
A node can read its own parachain record and use it as a bootnode in its network.

For offchain messaging, a node reads the record of every parachain it fetches payloads from.

The receivers drop values longer than 4096 bytes or that fail to decode. The addresses that contain non-global IP addresses 
are ignored on production networks.

For each parachain, the node adds a runtime `Kademlia` in client mode when it wants to fetch offchain messages.
The multiaddresses are given to the individual `Kademlia` handler to populate the routing table.
Then subsequent `FIND_NODE` queries will dial the provided bootnodes and discover further peers on the parachain network.

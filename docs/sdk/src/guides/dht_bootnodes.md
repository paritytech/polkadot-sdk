# DHT Bootnodes (RFC-8)

The "DHT bootnodes" mechanism, as defined in [RFC-0008: Store parachain bootnodes in relay chain
DHT](https://polkadot-fellows.github.io/RFCs/approved/0008-parachain-bootnodes-dht.html)
and implemented in Polkadot, enables parachain nodes to bootstrap without requiring hardcoded
bootnode addresses in the chainspec.

## How It Works

This mechanism, enabled by default, allows any parachain node to serve as a bootnode. In each
epoch, 20 parachain nodes are selected as bootnodes based on the proximity of their relay chain
peer IDs to the parachain key for that epoch. These selected nodes register themselves in the relay
chain's Kademlia DHT as [_content providers._](
https://github.com/libp2p/specs/tree/master/kad-dht#content-provider-advertisement-and-discovery)
Other nodes can then discover and query them to obtain the multiaddresses of their parachain
instances.

## Information for Parachain Operators

The DHT bootnode mechanism simplifies parachain deployment by removing the need for dedicated
bootnodes and hardcoded addresses in the chainspec. It also reduces the risk of single points
of failure if predefined bootnodes become unreachable.

However, since this feature is relatively new, high-value parachains are still advised to include
a set of dedicated bootnodes in the chainspec as a fallback mechanism. Also, the bootnodes
specified via the `--bootnodes` command-line option are always used.

## Command-Line Options

There are two independent CLI options controlling the mechanism:

- `--no-dht-bootnode` prevents a node from acting as a DHT bootnode.
- `--no-dht-bootnode-discovery` disables discovery of other parachain nodes via the DHT bootnode
  mechanism.


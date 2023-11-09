# Bridge Relayers Pallet

The pallet serves as a storage for pending bridge relayer rewards. Any runtime component may register reward
to some relayer for doing some useful job at some messages lane. Later, the relayer may claim its rewards
using the `claim_rewards` call.

The reward payment procedure is abstracted from the pallet code. One of possible implementations, is the
[`PayLaneRewardFromAccount`](../../primitives/relayers/src/lib.rs), which just does a `Currency::transfer`
call to relayer account from the relayer-rewards account, determined by the message lane id.

We have two examples of how this pallet is used in production. Rewards are registered at the target chain to
compensate fees of message delivery transactions (and linked finality delivery calls). At the source chain, rewards
are registered during delivery confirmation transactions. You may find more information about that in the
[Kusama <> Polkadot bridge](../../docs/polkadot-kusama-bridge-overview.md) documentation.

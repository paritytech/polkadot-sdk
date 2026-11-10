# HRMP channel deposits

HRMP stays on the relay chain. Only the channel deposits are held on another chain.

- `relay/` — `pallet-hrmp-relay`: runs on the relay chain. It is the `hrmp` pallet's deposit
  provider: it asks `pallet-hrmp-para` to set or release deposits and hands the answers back.
- `para/` — `pallet-hrmp-para`: runs on the chain that holds the deposits. It holds and releases
  what the relay chain asks, from the paying para's sovereign account.
- `primitives/` — `hrmp-primitives`: the wire types between the two.

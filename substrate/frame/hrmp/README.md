# HRMP channels

HRMP stays on the relay chain. Only the channel deposits are held elsewhere, on a parachain.

- `relay/` — `pallet-hrmp-relay`: runs on the relay chain. Serves a parachain's own HRMP requests
  (`relay_request`), and carries deposit holds and releases to the parachain that holds them.
- `para/` — `pallet-hrmp-para`: runs on the parachain that holds the deposits. Holds and releases
  what the relay chain asks, against a channel and side.
- `primitives/` — `hrmp-primitives`: the wire types between the two, and the para-facing
  `ParaRequest`.

The relay chain's `hrmp` pallet takes its deposits through `Config::ChannelDeposits`.
`ReserveDeposits` reserves them on the relay chain, as before.

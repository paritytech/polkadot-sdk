# Assets Parachain

Implementation of _Statemint_, a blockchain to support generic assets in the Polkadot and Kusama
networks.

Statemint allows users to:

- Deploy promise-backed assets, both fungible and non-fungible, with a DOT/KSM deposit.
- Set admin roles to manage assets and asset classes.
- Register assets as "self-sufficient" if the Relay Chain agrees, i.e. gain the ability for an
  asset to justify the existance of accounts sans DOT/KSM.
- Pay transaction fees using sufficient assets.
- Transfer (and approve transfer) assets.
- Interact with the chain via its transactional API or XCM.

Statemint must stay fully aligned with the Relay Chain it is connected to. As such, it will accept
the Relay Chain's governance origins as its own.

See
[the article on Statemint as common good parachain](https://www.parity.io/blog/statemint-generic-assets-chain-proposing-a-common-good-parachain-to-polkadot-governance/)
for a higher level description.

Wallets, custodians, etc. should see
[the Polkadot Wiki's Integration Guide](https://wiki.polkadot.network/docs/build-integrate-assets)
for details about support.

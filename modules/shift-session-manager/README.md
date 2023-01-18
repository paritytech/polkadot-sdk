# Shift Session Manager Pallet

**THIS PALLET IS NOT INTENDED TO BE USED IN PRODUCTION**

The pallet does not provide any calls or runtime storage entries. It only provides implementation of the
`pallet_session::SessionManager`. This implementation, starting from session `3` selects two thirds of initial
validators and changes the set on every session. We are using it at our testnets ([Rialto](../../bin/rialto/) and
[Millau](../../bin/millau/)) to be sure that the set changes every session. On well-known production chains
(like Kusama and Polkadot) the alternative is the set of [nPoS](https://research.web3.foundation/en/latest/polkadot/NPoS/index.html)
pallets, which selects validators, based on their nominations.

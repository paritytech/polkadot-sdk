E2E tests concerning Polkadot Governance and the Collectives Parachain. The tests run by the Parachain Integration Tests
[tool](https://github.com/paritytech/parachains-integration-tests/).

# Requirements
The tests require some changes to the regular production runtime builds:

## RelayChain runtime
1. Alice has SUDO
2. Public Referenda `StakingAdmin`, `FellowshipAdmin` tracks settings (see the corresponding keys of the `TRACKS_DATA`
   constant in the `governance::tracks` module of the Relay Chain runtime crate):
``` yaml
prepare_period: 5 Block,
decision_period: 1 Block,
confirm_period: 1 Block,
min_enactment_period: 1 Block,
```

## Collectives runtime
1. Fellowship Referenda `Fellows` track settings (see the corresponding key of the `TRACKS_DATA` constant in the
   `fellowship::tracks` module of the Collectives runtime crate):
``` yaml
prepare_period: 5 Block,
decision_period: 1 Block,
confirm_period: 1 Block,
min_enactment_period: 1 Block,
```

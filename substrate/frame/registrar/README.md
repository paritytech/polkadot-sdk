# Parachain registrar

- `para/` — `pallet-registrar-para`: user facing control-plane, runs on the Coretime chain.
- `relay/` — `pallet-registrar-relay`: receives messages from the controller, runs on the relay chain.
- `primitives/` — `registrar-primitives`: shared Para<->RC XCM message types.

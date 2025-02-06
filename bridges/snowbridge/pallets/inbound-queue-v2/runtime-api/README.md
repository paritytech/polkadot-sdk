# Ethereum Inbound Queue V2 Runtime API

This runtime API provides a "dry-run" interface for messages destined for the Snowbridge inbound queue. 

The core motivation is to allow UIs and relayers to query the cost of processing a message before actually submitting it on-chain. This helps users understand fees (e.g., for transaction finalization on the bridging parachain) and allows relayers to decide if it is profitable to relay.

# Overview
## Fee estimation (`dry_run`)

1. Converts a given Ethereum-based Message into XCM instructions via the configured MessageConverter.
2. Estimates the execution fee by combining:
- The on-chain weight cost of the submit extrinsic (in the Substrate runtime).
- A static XCM “prologue” fee for the instructions on AssetHub.

## Intended Consumers:

- Wallet/Frontend UI: To display approximate fees to end-users before they send a message from Ethereum.
- Relayers: To verify if relaying a particular message would be profitable once the reward (relayer fee) on Ethereum is accounted for.

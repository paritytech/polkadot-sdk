# Benchmarks

## Settlement Duration

Time taken for a cross-chain asset transfer to settle.

### Goerli → Rococo

* Minimum: 15min
* Average: 18min
* Maximum: 30min

### Rococo -> Goerli

Our rococo parachain is configured to create new message batches every block. In production, batching times will increase, so the figures below will also increase accordingly.&#x20;

* Minimum: 5min
* Average: 5min
* Maximum: 10min

## Relayer transaction fees

These are the transaction fees paid by relayers for delivering messages.

### Polkadot->Ethereum

The transaction fees for relaying a message bundle are highly variable, and depend on the bundle size and fluctuating gas costs.

For a bundle with a single message, the cost will range between roughly $10 to $20 in the current bear market. The cost can be broken down roughly as follows:

1. 50% of gas is used for bundle verification
2. 50% of gas is used for dispatch and execution of the single message

The overhead of bundle verification is nearly constant, so bundles of larger sizes will see proportionately more gas used for message execution rather than verification.

See this [spreadsheet](https://docs.google.com/spreadsheets/d/1bvTX1TmuAXPDVfLHbFisd\_xj-TRCYx7\_7jLYi\_hE7Zo) for the raw calculations.

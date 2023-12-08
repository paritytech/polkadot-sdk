# Operational Costs

To remain operational, the BEEFY light client must be updated with new BEEFY commitments. These commitments are emitted periodically by the relay chain, roughly every minute. A mandatory commitment is emitted at the start of every validator [session](https://wiki.polkadot.network/docs/maintain-polkadot-parameters#periods-of-common-actions-and-attributes) and must be provided to the light client.

It will be prohibitively expensive to submit updates every minute. So we envision that the rate of updates will be dynamic and influenced by user demand. Assuming current gas prices, the cost of operating the BEEFY client should be between $200,000 and $1,000,000 per year. For detailed calculations, see our [Cost Predictions](https://docs.google.com/spreadsheets/d/1QtxNtG4GE1IUaH204QFO6lObyAqLV9WCbmSYEopU18Q/edit?usp=sharing).

Our current implementation is not very optimized, as we have focused foremost on correctness and readability. However, we have identified several easy optimizations which can reduce the cost by at least 20% or more.

# Oracle Module

## Overview

The Oracle module provides a decentralized and trustworthy way to bring external, off-chain data onto the blockchain. It allows a configurable set of oracle operators to feed data, such as prices, into the system. This data can then be used by other pallets.

The module is designed to be flexible and can be configured to use different data sources and aggregation strategies.

### Key Concepts

*   **Oracle Operators**: A set of trusted accounts that are authorized to submit data to the oracle. The module uses the `frame_support::traits::SortedMembers` trait to manage the set of operators. This allows using pallets like `pallet-membership` to manage the oracle members.
*   **Data Feeds**: Operators feed data as key-value pairs. The `OracleKey` is used to identify the data being fed (e.g., a specific currency pair), and the `OracleValue` is the data itself (e.g., the price).
*   **Data Aggregation**: The module can be configured with a `CombineData` implementation to aggregate the raw values submitted by individual operators into a single, trusted value. A default implementation `DefaultCombineData` is provided, which takes the median of the values.
*   **Timestamped Data**: All data submitted to the oracle is timestamped, allowing consumers of the data to know how fresh it is.

## Interface

### Dispatchable Functions

*   `feed_values`: Allows an authorized oracle operator to submit a set of key-value data points.

### Public Functions

*   `get`: Returns the aggregated and timestamped value for a given key.
*   `get_all_values`: Returns all aggregated and timestamped values.
*   `read_raw_values`: Returns the raw, un-aggregated values for a given key from all oracle operators.

### Data Providers

The pallet implements the `DataProvider` and `DataProviderExtended` traits, allowing other pallets to easily consume the oracle data.

## Usage

To use the oracle pallet, you need to:

1.  **Add it to your runtime's `Cargo.toml`**.
2.  **Implement the `Config` trait** for the pallet in your runtime. This includes specifying:
    *   `OnNewData`: A hook to perform actions when new data is received.
    *   `CombineData`: The data aggregation strategy.
    *   `Time`: The time provider.
    *   `OracleKey`, `OracleValue`: The types for the data key and value.
    *   `RootOperatorAccountId`: An account with sudo-like permissions for the oracle.
    *   `Members`: The source of oracle operators.

3.  **Add the pallet to your runtime's `construct_runtime!` macro**.

Once configured, authorized operators can call `feed_values` to submit data, and other pallets can use the `DataProvider` trait to read the aggregated data.
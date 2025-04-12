# Optimistic Funding Pallet

A Substrate pallet that implements an optimistic funding mechanism for the Polkadot Ambassador Fellowship, allowing for decentralized allocation of funds based on community voting.

## Overview

The Optimistic Funding pallet enables a transparent and community-driven approach to funding allocation within the Polkadot ecosystem. It allows members to submit funding requests, vote on proposals they support, and ensures that financial rewards are tied to actual contributions rather than rank.

## Features

- **Funding Requests**: Members can submit requests for funding with descriptions and requested amounts
- **Voting Mechanism**: Community members can vote on funding requests to indicate support
- **Treasury Management**: Dedicated treasury account with controlled top-up and allocation
- **Period-based Processing**: Funding requests are processed within defined periods
- **Governance Integration**: Designed to work with existing governance systems like the Polkadot Ambassador Fellowship Program

## Extrinsics

- `submit_request`: Submit a new funding request with a description and amount
- `vote`: Vote for a funding request with a specific amount
- `cancel_vote`: Cancel a previously cast vote
- `top_up_treasury`: Add funds to the treasury (restricted to treasury origin)
- `reject_request`: Reject a funding request (restricted to treasury origin)
- `allocate_funds`: Manually allocate funds to a request (restricted to treasury origin)
- `set_period_end`: Set the end of the current funding period (restricted to treasury origin)

## Configuration

The pallet can be configured with the following parameters:

- `FundingPeriod`: The duration of a funding period (e.g. 28 days)
- `MinimumRequestAmount`: The minimum amount that can be requested
- `MaximumRequestAmount`: The maximum amount that can be requested
- `RequestDeposit`: The deposit required to make a funding request
- `MaxActiveRequests`: The maximum number of active funding requests
- `TreasuryOrigin`: The origin that can manage the treasury
- `PalletId`: The pallet ID used for deriving the treasury account

## Integration

This pallet is designed to be integrated with the Polkadot Ambassador Fellowship program, providing a mechanism for decentralized funding allocation. It works alongside other pallets such as `pallet_ranked_collective` and governance systems to create a comprehensive ecosystem for community-driven funding.

## Testing

The pallet includes comprehensive tests covering all functionalities:
- Basic request submission and voting
- Vote cancellation
- Treasury management
- Request rejection and fund allocation
- Period-based processing

## Benchmarking

Performance benchmarks are included for all extrinsics to ensure efficient on-chain execution.

## License

License: Apache-2.0

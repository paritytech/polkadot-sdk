<!-- markdown-link-check-disable -->
# Offchain Worker Example Pallet

The Offchain Worker Example: A simple pallet demonstrating
concepts, APIs and structures common to most offchain workers.

Run `cargo doc --package pallet-example-offchain-worker --open` to view this module's
documentation.

- [`pallet_example_offchain_worker::Trait`](./trait.Trait.html)
- [`Call`](./enum.Call.html)
- [`Pallet`](./struct.Pallet.html)
- [`ValidateUnsignedPriceSubmission`](./struct.ValidateUnsignedPriceSubmission.html)

**This pallet serves as an example showcasing Substrate off-chain worker and is not meant to be
used in production.**

## Overview

In this example we are going to build a very simplistic, naive and definitely NOT
production-ready oracle for BTC/USD price.
Offchain Worker (OCW) will be triggered after every block, fetch the current price
and prepare either signed or unsigned transaction to feed the result back on chain.
The on-chain logic will simply aggregate the results and store last `64` values to compute
the average price.
Additional logic in OCW is put in place to prevent spamming the network with both signed
and unsigned transactions. The pallet uses the `#[pallet::authorize]` attribute to validate
unsigned transactions, ensuring that only one unsigned transaction can be accepted per
`UnsignedInterval` blocks.

## Unsigned Transaction Validation

This pallet demonstrates how to validate unsigned transactions using the modern
`#[pallet::authorize]` attribute instead of the deprecated `ValidateUnsigned` trait.

The `#[pallet::authorize]` attribute is used on the unsigned transaction calls:
- `submit_price_unsigned` - Validates a simple unsigned transaction
- `submit_price_unsigned_with_signed_payload` - Validates an unsigned transaction with a signed payload

The authorization logic checks:
1. Block number is within the expected window (via `NextUnsignedAt` storage)
2. The price data is valid
3. For signed payloads, verifies the signature using the authority's public key

License: MIT-0

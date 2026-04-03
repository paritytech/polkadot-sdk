# Auction Pallet

## Overview

This pallet provides a generic framework for on-chain auctions.
It allows for the creation and management of auctions for any type of asset.
The main auction logic, like bid validation and ending, can be customized with the `AuctionHandler` trait.

## Features

- **Generic Auction Mechanism:** Can be used for auctioning any asset.
- **Customizable Logic:** The `AuctionHandler` trait allows for custom implementation of auction logic.
- **Scheduled Auctions:** Auctions can be scheduled to start at a future block number.
- **Automatic Auction Closing:** Auctions are automatically closed at their end block number in the `on_finalize` hook.

## How It Works

### Auction Lifecycle

1. **Creation:** An auction is created using the `new_auction` function from the `Auction` trait.
It can be configured with a start time and an optional end time.
2. **Bidding:** Once an auction has started, users can place bids using the `bid` extrinsic.
The `AuctionHandler` implementation validates each bid.
For example, it can enforce that a new bid must be higher than the current highest bid.
3. **Ending:** If an auction has an end time, it will be automatically concluded in the `on_finalize` hook of the block.
When an auction ends, `on_auction_ended` is called to handle the result, such as transferring the item and payment.

### `AuctionHandler` Trait

The `AuctionHandler` trait is the core of this pallet's customizability. It defines two main functions:

- `on_new_bid`: This function is called whenever a new bid is placed. It allows for custom validation logic.
It can also be used to extend the auction's duration if a bid is placed near the end time.
- `on_auction_ended`: This function is called when an auction concludes.
It is responsible for handling the final state of the auction, such as transferring assets and funds.

By implementing this trait, developers can define the specific rules and outcomes for their auctions.

## Usage

To use this pallet in a runtime, you need to:

1. Add it as a dependency in your runtime's `Cargo.toml`.
2. Implement the `Config` trait for the pallet.
3. Implement the `AuctionHandler` trait with your custom auction logic.
4. Include the pallet in your runtime's `construct_runtime!` macro.

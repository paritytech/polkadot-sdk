//! # XCM Cookbook

/// Setting up an [AssetTransactor](xcm_executor::traits::transact_asset::TransactAsset) to handle
/// the relay chain token.
/// Useful for a parachain that only wants to deal with DOT.
pub mod relay_token_transactor;

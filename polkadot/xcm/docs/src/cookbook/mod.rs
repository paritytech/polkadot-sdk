//! # XCM Cookbook
//!
//! A collection of XCM recipes.
//!
//! Each recipe is tested and explains all the code necessary to run it -- they're not just snippets to copy and paste.

/// Configuring a parachain that only uses the Relay Chain native token.
/// In the case of Polkadot, this recipe will show you how to launch a parachain with no native token -- dealing only on DOT.
pub mod relay_token_transactor;

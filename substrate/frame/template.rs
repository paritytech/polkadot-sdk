
//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/sudo)
//! [![polkadot]](https://polkadot.network)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! 
//! # Sudo Pallet
//!
//! A pallet to provide a way to execute privileged runtime calls using a specified sudo ("superuser
//! do") account.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! In Substrate blockchains pallets may contain dispatchable calls that can only be called at
//! the system level of the chain (i.e. dispatchables that require a `Root` origin).
//! Setting a privileged account called the _sudo key_ allows you to make such calls as an
//! extrinisic.
//!
//! Here's an example of a privileged function in another pallet:
//!
//! ### Examples
//!
//! 1. You can make a privileged runtime call using `sudo` with an account that matches the sudo
//!    key.
#![doc = docify::embed!("src/tests.rs", sudo_basics)]
//!
//! ## Low Level / Implementation Details
//! 
//! ...
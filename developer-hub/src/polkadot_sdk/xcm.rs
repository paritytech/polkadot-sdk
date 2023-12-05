//! # XCM
//!
//! XCM, or Cross-Consensus Messaging, is a **language** to communicate **intentions** between **consensus systems**.
//!
//! ## Overview
//!
//! XCM is a language angostic standard, whose specification lives in the [xcm format repo](https://github.com/paritytech/xcm-format).
//!
//! It enables different consensus systems to communicate with each other in an expressive manner.
//! Consensus systems include blockchains, smart contracts, and any other state machine that achieves consensus in some way.
//!
//! XCM is based on a virtual machine, the XCVM.
//! An XCM program is a series of instructions, which get executed one after the other by the virtual machine.
//! These instructions aim to encompass all major things users typically do in consensus systems.
//! There are instructions on asset transferring, teleporting, locking, among others.
//! New instructions are added via the [RFC process](https://github.com/paritytech/xcm-format/blob/master/proposals/0032-process.md).
//!
//! ## Implementation
//!
//! A ready-to-use Rust implementation lives in the [polkadot-sdk repo](https://github.com/paritytech/polkadot-sdk/tree/master/polkadot/xcm),
//! but will be moved to its own repo in the future.
//!
//! Its main components are:
//! - `src`: the definition of the basic types and instructions
//! - `xcm-executor`: an implementation of the virtual machine to execute instructions
//! - `pallet-xcm`: A FRAME pallet for interacting with the executor
//! - `xcm-builder`: a collection of types to configure the executor
//! - `xcm-simulator`: a playground for trying out different XCM programs and executor configurations
//!
//! ## Get started
//!
//! To learn how it works and to get started, go to the [XCM docs](https://paritytech.github.io/xcm-docs/).

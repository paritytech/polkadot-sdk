//! # XCM
//!
//! XCM, or Cross-Consensus Messaging, is a **language** to communicate **intentions** between
//! **consensus systems**.
//!
//! ## Overview
//!
//! XCM is a standard, whose specification lives in the [xcm format repo](https://github.com/paritytech/xcm-format).
//! It's agnostic both in programming language and blockchain platform, which means it could be used
//! in Rust in Polkadot, or in Go or C++ in any other platform like Cosmos or Ethereum.
//!
//! It enables different consensus systems to communicate with each other in an expressive manner.
//! Consensus systems include blockchains, smart contracts, and any other state machine that
//! achieves consensus in some way.
//!
//! XCM is executed on a virtual machine called the XCVM.
//! Scripts can be written with the XCM language, which are often called XCMs, messages or XCM
//! programs. Each program is a series of instructions, which get executed one after the other by
//! the virtual machine. These instructions aim to encompass all major things users typically do in
//! consensus systems. There are instructions on asset transferring, teleporting, locking, among
//! others. New instructions are added and changes to the XCVM are made via the [RFC process](https://github.com/paritytech/xcm-format/blob/master/proposals/0032-process.md).
//!
//! ## In Polkadot SDK
//!
//! The Polkadot SDK allows for easily deploying sovereign blockchains from scratch, all very
//! customizable. Dealing with many heterogeneous blockchains can be cumbersome.
//! XCM allows all these blockchains to communicate with an agreed-upon language.
//! As long as an implementation of the XCVM is implemented, the same XCM program can be executed in
//! all blockchains and perform the same task.
//!
//! ## Implementation
//!
//! A ready-to-use Rust implementation lives in the [polkadot-sdk repo](https://github.com/paritytech/polkadot-sdk/tree/master/polkadot/xcm),
//! but will be moved to its own repo in the future.
//!
//! Its main components are:
//! - `src`: the definition of the basic types and instructions
//! - [`xcm-executor`](https://paritytech.github.io/polkadot-sdk/master/staging_xcm_executor/struct.XcmExecutor.html):
//!   an implementation of the virtual machine to execute instructions
//! - `pallet-xcm`: A FRAME pallet for interacting with the executor
//! - `xcm-builder`: a collection of types to configure the executor
//! - `xcm-simulator`: a playground for trying out different XCM programs and executor
//!   configurations
//!
//! ## Example
//!
//! To perform the very usual operation of transferring assets, the following XCM program can be
//! used:
#![doc = docify::embed!("src/polkadot_sdk/xcm.rs", example_transfer)]
//!
//! ## Get started
//!
//! To learn how it works and to get started, go to the [XCM docs](https://paritytech.github.io/xcm-docs/).

#[cfg(test)]
mod tests {
	use xcm::latest::prelude::*;

	#[docify::export]
	#[test]
	fn example_transfer() {
		let _transfer_program = Xcm::<()>(vec![
			WithdrawAsset((Here, 100u128).into()),
			BuyExecution { fees: (Here, 100u128).into(), weight_limit: Unlimited },
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { id: [0u8; 32].into(), network: None }.into(),
			},
		]);
	}
}

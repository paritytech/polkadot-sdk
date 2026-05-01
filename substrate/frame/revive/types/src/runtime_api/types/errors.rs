// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Stable wire-owned error types returned by versioned runtime API payloads.

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;

define_versioned_type! {
	/// Version 1 of a dispatch failure returned by dry-run contract execution.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub enum DispatchErrorV1 {
		/// An unspecified dispatch error.
		Other,
		/// Runtime lookup failed.
		CannotLookup,
		/// The call used an origin that is not permitted for the operation.
		BadOrigin,
		/// Pallet-specific error identified by pallet index and encoded pallet error bytes.
		Module(ModuleErrorV1),
		/// The account still has consumers and cannot be destroyed.
		ConsumerRemaining,
		/// The account has no providers and cannot be created.
		NoProviders,
		/// The account has too many consumers and cannot be created.
		TooManyConsumers,
		/// Token-related failure.
		Token(TokenErrorV1),
		/// Arithmetic failure.
		Arithmetic(ArithmeticErrorV1),
		/// Transactional-storage failure.
		Transactional(TransactionalErrorV1),
		/// Resources were exhausted while processing the dispatch.
		Exhausted,
		/// Runtime state was corrupt.
		Corruption,
		/// A required resource was unavailable.
		Unavailable,
		/// Root origin was not allowed.
		RootNotAllowed,
		/// Trie-related failure.
		Trie(TrieErrorV1),
	}
}

define_versioned_type! {
	/// Version 1 of a pallet-specific dispatch failure.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub struct ModuleErrorV1 {
		/// Pallet index matching the runtime metadata pallet index.
		pub index: u8,
		/// Pallet-specific encoded error payload.
		pub error: [u8; 4],
	}
}

define_versioned_type! {
	/// Version 1 of token-related dispatch failures.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub enum TokenErrorV1 {
		/// Funds are unavailable.
		FundsUnavailable,
		/// The balance is the only provider reference and cannot be removed.
		OnlyProvider,
		/// Account cannot exist with the provided funds.
		BelowMinimum,
		/// Account cannot be created.
		CannotCreate,
		/// The asset is unknown.
		UnknownAsset,
		/// Funds exist but are frozen.
		Frozen,
		/// Operation is unsupported by the asset.
		Unsupported,
		/// Account cannot be created for a held balance.
		CannotCreateHold,
		/// Withdrawal would cause unwanted account loss.
		NotExpendable,
		/// Account cannot receive the assets.
		Blocked,
	}
}

define_versioned_type! {
	/// Version 1 of arithmetic dispatch failures.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub enum ArithmeticErrorV1 {
		/// Arithmetic underflow.
		Underflow,
		/// Arithmetic overflow.
		Overflow,
		/// Division by zero.
		DivisionByZero,
	}
}

define_versioned_type! {
	/// Version 1 of transactional-storage dispatch failures.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub enum TransactionalErrorV1 {
		/// Too many transactional layers have been spawned.
		LimitReached,
		/// A transactional layer was expected, but does not exist.
		NoLayer,
	}
}

define_versioned_type! {
	/// Version 1 of trie-related dispatch failures.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub enum TrieErrorV1 {
		/// The state root is not in the database.
		InvalidStateRoot,
		/// A trie item was not found in the database.
		IncompleteDatabase,
		/// A value was found with a key that is not byte-aligned.
		ValueAtIncompleteKey,
		/// A corrupt trie item was encountered.
		DecoderError,
		/// The hash does not match the expected value.
		InvalidHash,
		/// The proof contains duplicate keys.
		DuplicateKey,
		/// The proof contains extraneous nodes.
		ExtraneousNode,
		/// The proof contains extraneous values.
		ExtraneousValue,
		/// The proof contains extraneous hash references.
		ExtraneousHashReference,
		/// The proof contains an invalid child reference.
		InvalidChildReference,
		/// The proof indicates a value mismatch.
		ValueMismatch,
		/// The proof is incomplete.
		IncompleteProof,
		/// The root hash computed from the proof is incorrect.
		RootMismatch,
		/// One of the proof nodes could not be decoded.
		DecodeError,
	}
}

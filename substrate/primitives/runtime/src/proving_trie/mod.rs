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

//! Types for creating a proving trie.

pub mod compact_base16;

use crate::{Decode, Encode, MaxEncodedLen, TypeInfo};
#[cfg(feature = "serde")]
use crate::{Deserialize, Serialize};

use sp_trie::{trie_types::TrieError as SpTrieError, VerifyError};

/// A runtime friendly error type for tries.
#[derive(Eq, PartialEq, Clone, Copy, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TrieError {
	/* From TrieError */
	/// Attempted to create a trie with a state root not in the DB.
	InvalidStateRoot,
	/// Trie item not found in the database,
	IncompleteDatabase,
	/// A value was found in the trie with a nibble key that was not byte-aligned.
	ValueAtIncompleteKey,
	/// Corrupt Trie item.
	DecoderError,
	/// Hash is not value.
	InvalidHash,
	/* From VerifyError */
	/// The statement being verified contains multiple key-value pairs with the same key.
	DuplicateKey,
	/// The proof contains at least one extraneous node.
	ExtraneousNode,
	/// The proof contains at least one extraneous value which should have been omitted from the
	/// proof.
	ExtraneousValue,
	/// The proof contains at least one extraneous hash reference the should have been omitted.
	ExtraneousHashReference,
	/// The proof contains an invalid child reference that exceeds the hash length.
	InvalidChildReference,
	/// The proof indicates that an expected value was not found in the trie.
	ValueMismatch,
	/// The proof is missing trie nodes required to verify.
	IncompleteProof,
	/// The root hash computed from the proof is incorrect.
	RootMismatch,
	/// One of the proof nodes could not be decoded.
	DecodeError,
}

impl<T> From<SpTrieError<T>> for TrieError {
	fn from(error: SpTrieError<T>) -> Self {
		match error {
			SpTrieError::InvalidStateRoot(..) => Self::InvalidStateRoot,
			SpTrieError::IncompleteDatabase(..) => Self::IncompleteDatabase,
			SpTrieError::ValueAtIncompleteKey(..) => Self::ValueAtIncompleteKey,
			SpTrieError::DecoderError(..) => Self::DecoderError,
			SpTrieError::InvalidHash(..) => Self::InvalidHash,
		}
	}
}

impl<T, U> From<VerifyError<T, U>> for TrieError {
	fn from(error: VerifyError<T, U>) -> Self {
		match error {
			VerifyError::DuplicateKey(..) => Self::DuplicateKey,
			VerifyError::ExtraneousNode => Self::ExtraneousNode,
			VerifyError::ExtraneousValue(..) => Self::ExtraneousValue,
			VerifyError::ExtraneousHashReference(..) => Self::ExtraneousHashReference,
			VerifyError::InvalidChildReference(..) => Self::InvalidChildReference,
			VerifyError::ValueMismatch(..) => Self::ValueMismatch,
			VerifyError::IncompleteProof => Self::IncompleteProof,
			VerifyError::RootMismatch(..) => Self::RootMismatch,
			VerifyError::DecodeError(..) => Self::DecodeError,
		}
	}
}

impl From<TrieError> for &'static str {
	fn from(e: TrieError) -> &'static str {
		match e {
			TrieError::InvalidStateRoot => "The state root is not in the database.",
			TrieError::IncompleteDatabase => "A trie item was not found in the database.",
			TrieError::ValueAtIncompleteKey =>
				"A value was found with a key that is not byte-aligned.",
			TrieError::DecoderError => "A corrupt trie item was encountered.",
			TrieError::InvalidHash => "The hash does not match the expected value.",
			TrieError::DuplicateKey => "The proof contains duplicate keys.",
			TrieError::ExtraneousNode => "The proof contains extraneous nodes.",
			TrieError::ExtraneousValue => "The proof contains extraneous values.",
			TrieError::ExtraneousHashReference => "The proof contains extraneous hash references.",
			TrieError::InvalidChildReference => "The proof contains an invalid child reference.",
			TrieError::ValueMismatch => "The proof indicates a value mismatch.",
			TrieError::IncompleteProof => "The proof is incomplete.",
			TrieError::RootMismatch => "The root hash computed from the proof is incorrect.",
			TrieError::DecodeError => "One of the proof nodes could not be decoded.",
		}
	}
}

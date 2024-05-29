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

//! Try-runtime specific traits and types.

pub mod decode_entire_state;
pub use decode_entire_state::{TryDecodeEntireStorage, TryDecodeEntireStorageError};

use super::StorageInstance;

use impl_trait_for_tuples::impl_for_tuples;
use sp_arithmetic::traits::AtLeast32BitUnsigned;
use sp_runtime::TryRuntimeError;
use sp_std::prelude::*;

/// Which state tests to execute.
#[derive(codec::Encode, codec::Decode, Clone, scale_info::TypeInfo)]
pub enum Select {
	/// None of them.
	None,
	/// All of them.
	All,
	/// Run a fixed number of them in a round robin manner.
	RoundRobin(u32),
	/// Run only logic whose identifier is included in the given list.
	///
	/// For pallets, their identifiers are obtained from [`super::PalletInfoAccess`].
	Only(Vec<Vec<u8>>),
}

impl Select {
	/// Whether to run any checks at all.
	pub fn any(&self) -> bool {
		!matches!(self, Select::None)
	}
}

impl Default for Select {
	fn default() -> Self {
		Select::None
	}
}

impl sp_std::fmt::Debug for Select {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		match self {
			Select::RoundRobin(x) => write!(f, "RoundRobin({})", x),
			Select::Only(x) => write!(
				f,
				"Only({:?})",
				x.iter()
					.map(|x| sp_std::str::from_utf8(x).unwrap_or("<invalid?>"))
					.collect::<Vec<_>>(),
			),
			Select::All => write!(f, "All"),
			Select::None => write!(f, "None"),
		}
	}
}

#[cfg(feature = "std")]
impl sp_std::str::FromStr for Select {
	type Err = &'static str;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"all" | "All" => Ok(Select::All),
			"none" | "None" => Ok(Select::None),
			_ => {
				if s.starts_with("rr-") {
					let count = s
						.split_once('-')
						.and_then(|(_, count)| count.parse::<u32>().ok())
						.ok_or("failed to parse count")?;
					Ok(Select::RoundRobin(count))
				} else {
					let pallets = s.split(',').map(|x| x.as_bytes().to_vec()).collect::<Vec<_>>();
					Ok(Select::Only(pallets))
				}
			},
		}
	}
}

/// Select which checks should be run when trying a runtime upgrade upgrade.
#[derive(codec::Encode, codec::Decode, Clone, Debug, Copy, scale_info::TypeInfo)]
pub enum UpgradeCheckSelect {
	/// Run no checks.
	None,
	/// Run the `try_state`, `pre_upgrade` and `post_upgrade` checks.
	All,
	/// Run the `pre_upgrade` and `post_upgrade` checks.
	PreAndPost,
	/// Run the `try_state` checks.
	TryState,
}

impl UpgradeCheckSelect {
	/// Whether the pre- and post-upgrade checks are selected.
	pub fn pre_and_post(&self) -> bool {
		matches!(self, Self::All | Self::PreAndPost)
	}

	/// Whether the try-state checks are selected.
	pub fn try_state(&self) -> bool {
		matches!(self, Self::All | Self::TryState)
	}

	/// Whether to run any checks at all.
	pub fn any(&self) -> bool {
		!matches!(self, Self::None)
	}
}

#[cfg(feature = "std")]
impl core::str::FromStr for UpgradeCheckSelect {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_lowercase().as_str() {
			"none" => Ok(Self::None),
			"all" => Ok(Self::All),
			"pre-and-post" => Ok(Self::PreAndPost),
			"try-state" => Ok(Self::TryState),
			_ => Err("Invalid CheckSelector"),
		}
	}
}

pub trait TryStateLogic<BlockNumber> {
	fn try_state(_: BlockNumber) -> Result<(), TryRuntimeError>;
}

#[cfg_attr(all(not(feature = "tuples-96"), not(feature = "tuples-128")), impl_for_tuples(64))]
#[cfg_attr(all(feature = "tuples-96", not(feature = "tuples-128")), impl_for_tuples(96))]
#[cfg_attr(all(feature = "tuples-128"), impl_for_tuples(128))]
impl<BlockNumber> TryStateLogic<BlockNumber> for Tuple
where
	BlockNumber: Clone + sp_std::fmt::Debug + AtLeast32BitUnsigned,
{
	fn try_state(n: BlockNumber) -> Result<(), TryRuntimeError> {
		let mut errors = Vec::<TryRuntimeError>::new();

		for_tuples!(#(
			if let Err(err) = Tuple::try_state(n.clone()) {
				errors.push(err);
			}
		)*);

		if !errors.is_empty() {
			return Err("Detected errors while executing `try_state` checks. See logs for more \
						info."
				.into());
		}

		Ok(())
	}
}

pub trait IdentifiableTryStateLogic<BlockNumber>: TryStateLogic<BlockNumber> {
	fn matches_id(_id: &[u8]) -> bool;
}

#[cfg_attr(all(not(feature = "tuples-96"), not(feature = "tuples-128")), impl_for_tuples(64))]
#[cfg_attr(all(feature = "tuples-96", not(feature = "tuples-128")), impl_for_tuples(96))]
#[cfg_attr(all(feature = "tuples-128"), impl_for_tuples(128))]
impl<BlockNumber> IdentifiableTryStateLogic<BlockNumber> for Tuple
where
	BlockNumber: Clone + sp_std::fmt::Debug + AtLeast32BitUnsigned,
{
	for_tuples!( where #( Tuple: TryStateLogic<BlockNumber> )* );
	fn matches_id(id: &[u8]) -> bool {
		for_tuples!( #( if Tuple::matches_id(id) { return true; } )* );
		return false;
	}
}

/// Execute some checks to ensure the internal state of a pallet is consistent.
///
/// Usually, these checks should check all of the invariants that are expected to be held on all of
/// the storage items of your pallet.
///
/// This hook should not alter any storage.
pub trait TryState<BlockNumber> {
	/// Execute the state checks.
	fn try_state(_: BlockNumber, _: Select) -> Result<(), TryRuntimeError>;
}

impl<BlockNumber> TryState<BlockNumber> for () {
	fn try_state(_: BlockNumber, _: Select) -> Result<(), TryRuntimeError> {
		Ok(())
	}
}

impl<BlockNumber, T1, T2> TryState<BlockNumber> for (T1, T2)
where
	BlockNumber: Clone + sp_std::fmt::Debug + AtLeast32BitUnsigned,
	T1: IdentifiableTryStateLogic<BlockNumber>,
	T2: IdentifiableTryStateLogic<BlockNumber>,
{
	fn try_state(n: BlockNumber, targets: Select) -> Result<(), TryRuntimeError> {
		match targets {
			Select::None => Ok(()),
			Select::All => {
				let mut errors = Vec::<TryRuntimeError>::new();

				if let Err(err) = T1::try_state(n.clone()) {
					errors.push(err);
				}
				if let Err(err) = T2::try_state(n.clone()) {
					errors.push(err);
				}

				if !errors.is_empty() {
					log::error!(
						target: "try-runtime",
						"Detected errors while executing `try_state`:",
					);

					errors.iter().for_each(|err| {
						log::error!(
							target: "try-runtime",
							"{:?}",
							err
						);
					});

					return Err(
						"Detected errors while executing `try_state` checks. See logs for more \
						info."
							.into(),
					);
				}

				Ok(())
			},
			Select::RoundRobin(len) => {
				let functions: &[fn(BlockNumber) -> Result<(), TryRuntimeError>] =
					&[T1::try_state, T2::try_state];
				let skip = n.clone() % (functions.len() as u32).into();
				let skip: u32 =
					skip.try_into().unwrap_or_else(|_| sp_runtime::traits::Bounded::max_value());
				let mut result = Ok(());
				for try_state_fn in functions.iter().cycle().skip(skip as usize).take(len as usize)
				{
					result = result.and(try_state_fn(n.clone()));
				}
				result
			},
			Select::Only(ref try_state_identifiers) => {
				let try_state_fns: &[(
					fn(&[u8]) -> bool,
					fn(BlockNumber) -> Result<(), TryRuntimeError>,
				)] = &[(T1::matches_id, T1::try_state), (T2::matches_id, T2::try_state)];

				let mut result = Ok(());
				try_state_identifiers.iter().for_each(|id| {
					if let Some((_, try_state_fn)) =
						try_state_fns.iter().find(|(eq_logic, _)| eq_logic(id.as_slice()))
					{
						result = result.and(try_state_fn(n.clone()));
					} else {
						log::warn!(
							"Try-state logic with identifier {:?} not found",
							sp_std::str::from_utf8(id).unwrap_or_default()
						);
					}
				});

				result
			},
		}
	}
}

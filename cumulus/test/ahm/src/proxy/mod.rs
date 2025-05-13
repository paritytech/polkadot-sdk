// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

pub mod basic_still_works;
pub mod whale_watching;

pub use basic_still_works::ProxyBasicWorks;
pub use whale_watching::ProxyWhaleWatching;

use crate::porting_prelude::*;

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Defensive},
};
use frame_system::pallet_prelude::*;
use hex_literal::hex;
use pallet_ah_migrator::types::AhMigrationCheck;
use pallet_rc_migrator::types::{RcMigrationCheck, ToPolkadotSs58};
use sp_runtime::{
	traits::{Dispatchable, TryConvert},
	AccountId32,
};
use std::{collections::BTreeMap, str::FromStr};

/// Intent based permission.
///
/// Should be a superset of all possible proxy types.
#[derive(Clone, PartialEq, Eq, RuntimeDebug)]
pub enum Permission {
	Any,
	NonTransfer,
	Governance,
	Staking,
	CancelProxy,
	Auction,
	NominationPools,
	ParaRegistration,
}

// Implementation for the Polkadot runtime. Will need more for Kusama and Westend in the future.
impl TryConvert<rc_proxy_definition::ProxyType, Permission> for Permission {
	fn try_convert(
		proxy: rc_proxy_definition::ProxyType,
	) -> Result<Self, rc_proxy_definition::ProxyType> {
		use rc_proxy_definition::ProxyType::*;

		Ok(match proxy {
			Any => Permission::Any,
			NonTransfer => Permission::NonTransfer,
			Governance => Permission::Governance,
			Staking => Permission::Staking,
			CancelProxy => Permission::CancelProxy,
			Auction => Permission::Auction,
			NominationPools => Permission::NominationPools,
			ParaRegistration => Permission::ParaRegistration,

			#[cfg(feature = "ahm-westend")]
			SudoBalances | IdentityJudgement => return Err(proxy),
		})
	}
}

impl TryConvert<asset_hub_polkadot_runtime::ProxyType, Permission> for Permission {
	fn try_convert(
		proxy: asset_hub_polkadot_runtime::ProxyType,
	) -> Result<Self, asset_hub_polkadot_runtime::ProxyType> {
		use asset_hub_polkadot_runtime::ProxyType::*;

		Ok(match proxy {
			Any => Permission::Any,
			NonTransfer => Permission::NonTransfer,
			Governance => Permission::Governance,
			Staking => Permission::Staking,
			CancelProxy => Permission::CancelProxy,
			Auction => Permission::Auction,
		})
	}
}

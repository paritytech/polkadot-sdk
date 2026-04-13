// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! XCM executor plumbing for **simulating** Snowbridge v2 Ethereum outbound blobs on Bridge Hub
//! ([`crate::v2::ExecuteBeforeSnowbridgeV2BlobExport`] dry-run).

use core::marker::PhantomData;

use frame_support::{
	traits::{
		tokens::imbalance::{
			ImbalanceAccounting, UnsafeConstructorDestructor, UnsafeManualAccounting,
		},
		Get, ProcessMessageError,
	},
	weights::Weight,
};
use sp_std::{boxed::Box, prelude::*};
use xcm::{
	latest::{Asset as XcmAsset, Error as XcmError, Fungibility, Junction, XcmContext},
	prelude::*,
};
use xcm_executor::{
	traits::{Properties, ShouldExecute, TransactAsset, WeightTrader},
	AssetsInHolding,
};

use crate::v2::{
	converter::snowbridge_v2_outbound_xcm_shape,
	exporter::snowbridge_v2_instructions_contain_alias_origin,
};

/// Minimal [`ImbalanceAccounting`] for [`AssetsInHolding`] credits used only by
/// [`EthereumSimulationAssetTransactor`] (no real balances).
struct SimulationFungibleCredit(u128);

impl UnsafeConstructorDestructor<u128> for SimulationFungibleCredit {
	fn unsafe_clone(&self) -> Box<dyn ImbalanceAccounting<u128>> {
		Box::new(Self(self.0))
	}
	fn forget_imbalance(&mut self) -> u128 {
		let amt = self.0;
		self.0 = 0;
		amt
	}
}

impl UnsafeManualAccounting<u128> for SimulationFungibleCredit {
	fn saturating_subsume(&mut self, mut other: Box<dyn ImbalanceAccounting<u128>>) {
		self.0 = self.0.saturating_add(other.forget_imbalance());
	}
}

impl ImbalanceAccounting<u128> for SimulationFungibleCredit {
	fn amount(&self) -> u128 {
		self.0
	}
	fn saturating_take(&mut self, amount: u128) -> Box<dyn ImbalanceAccounting<u128>> {
		let taken = self.0.min(amount);
		self.0 -= taken;
		Box::new(Self(taken))
	}
}

fn simulation_asset_to_holding(asset: XcmAsset) -> AssetsInHolding {
	let mut holding = AssetsInHolding::new();
	match asset.fun {
		Fungibility::Fungible(amount) => {
			holding.fungible.insert(asset.id, Box::new(SimulationFungibleCredit(amount)));
		},
		Fungibility::NonFungible(instance) => {
			holding.non_fungible.insert((asset.id, instance));
		},
	}
	holding
}

/// Snowbridge v2 Ethereum destinations use an `AccountKey20` junction for the beneficiary address.
/// Match that here so invalid shapes fail [`TransactAsset::deposit_asset`] and simulated XCM ends
/// with assets still in holding (then the runtime’s configured [`xcm_executor::Config::AssetTrap`]
/// records them).
pub fn ethereum_simulation_deposit_beneficiary_valid(who: &Location) -> bool {
	matches!(who.last(), Some(Junction::AccountKey20 { .. }))
}

/// Asset transactor for Snowbridge Ethereum **export simulation** on Bridge Hub.
///
/// Withdraw / mint / internal transfer are bookkeeping-only (no real balances). [`DepositAsset`]
/// only succeeds when the beneficiary ends with
/// [`AccountKey20`], matching Snowbridge’s Ethereum-facing shape; otherwise
/// assets stay in holding for trapping.
pub struct EthereumSimulationAssetTransactor;
impl TransactAsset for EthereumSimulationAssetTransactor {
	fn withdraw_asset(
		what: &XcmAsset,
		_who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(simulation_asset_to_holding(what.clone()))
	}

	fn deposit_asset(
		what: AssetsInHolding,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(), (AssetsInHolding, XcmError)> {
		if !ethereum_simulation_deposit_beneficiary_valid(who) {
			return Err((
				what,
				XcmError::FailedToTransactAsset(
					"Ethereum DepositAsset beneficiary must end with AccountKey20",
				),
			));
		}
		drop(what);
		Ok(())
	}

	fn mint_asset(what: &XcmAsset, _context: &XcmContext) -> Result<AssetsInHolding, XcmError> {
		Ok(simulation_asset_to_holding(what.clone()))
	}

	fn internal_transfer_asset(
		what: &XcmAsset,
		_from: &Location,
		_to: &Location,
		_context: &XcmContext,
	) -> Result<XcmAsset, XcmError> {
		Ok(what.clone())
	}
}

/// [`ShouldExecute`] barrier for the Ethereum XCM simulation executor when
/// [`crate::v2::ExecuteBeforeSnowbridgeV2BlobExport`] runs the **inner** Ethereum `ExportMessage`
/// blob. That blob does not use `UnpaidExecution`; fee-free simulation is provided separately by
/// [`EthereumExecutionFreeTrader`] and the runtime’s configured fee manager (e.g.
/// [`xcm_executor::traits::WaiveDeliveryFees`]).
///
/// Snowbridge v2 outbound blobs are recognized by an [`AliasOrigin`] instruction (same as
/// [`crate::v2::EthereumBlobExporter`]). Those programs must match
/// [`snowbridge_v2_outbound_xcm_shape`]. Legacy Snowbridge v1 exports contain no `AliasOrigin`;
/// they are rejected here while the v1 [`crate::v1::EthereumBlobExporter`] still validates on
/// enqueue.
pub struct EthereumExportSimulationBarrier<EthereumNetwork: Get<NetworkId>>(
	PhantomData<EthereumNetwork>,
);

impl<EthereumNetwork: Get<NetworkId>> ShouldExecute
	for EthereumExportSimulationBarrier<EthereumNetwork>
{
	fn should_execute<RuntimeCall>(
		_origin: &Location,
		instructions: &mut [Instruction<RuntimeCall>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		if snowbridge_v2_instructions_contain_alias_origin(instructions) {
			snowbridge_v2_outbound_xcm_shape(instructions, EthereumNetwork::get())
		} else {
			Err(ProcessMessageError::Unsupported)
		}
	}
}

/// Buys execution weight without touching real fee balances (typically paired with
/// [`xcm_executor::traits::WaiveDeliveryFees`] in the runtime config).
#[derive(Clone)]
pub struct EthereumExecutionFreeTrader;
impl WeightTrader for EthereumExecutionFreeTrader {
	fn new() -> Self {
		Self
	}

	fn buy_weight(
		&mut self,
		_weight: Weight,
		_payment: AssetsInHolding,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, (AssetsInHolding, XcmError)> {
		Ok(AssetsInHolding::new())
	}
}

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Shared Snowbridge v2 outbound XCM prefix parsing used by [`super::shape`] and
//! [`super::convert::XcmConverter`].

use xcm::prelude::*;

pub(crate) fn network_matches_ethereum(
	network: &Option<NetworkId>,
	ethereum_network: NetworkId,
) -> bool {
	network.map_or(true, |n| n == ethereum_network)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteFeeParseErr {
	ExpectedWithdrawAsset,
	/// Withdraw parsed but `PayFees` is missing (matches iterator `next()` running out).
	UnexpectedEndAfterWithdraw,
	AssetResolutionFailed,
	InvalidFeeAsset,
}

/// First two instructions: `WithdrawAsset` (single ETH fee) + `PayFees`.
///
/// Returns `(instructions_consumed, fee_amount_from_pay_fees)`.
pub(crate) fn parse_remote_fee_section<Call>(
	instructions: &[Instruction<Call>],
) -> Result<(usize, u128), RemoteFeeParseErr> {
	let mut i = 0;
	let fee_assets = match instructions.get(i) {
		Some(Instruction::WithdrawAsset(assets)) => assets,
		_ => return Err(RemoteFeeParseErr::ExpectedWithdrawAsset),
	};
	if fee_assets.len() != 1 {
		return Err(RemoteFeeParseErr::AssetResolutionFailed);
	}
	let reserved_fee_asset =
		fee_assets.inner().first().ok_or(RemoteFeeParseErr::AssetResolutionFailed)?;
	let (reserved_fee_asset_id, reserved_fee_amount) = match reserved_fee_asset {
		Asset { id: asset_id, fun: Fungible(amount) } => (asset_id, *amount),
		_ => return Err(RemoteFeeParseErr::AssetResolutionFailed),
	};
	i += 1;

	let fee_asset = match instructions.get(i) {
		Some(Instruction::PayFees { asset: fee }) => fee,
		None => return Err(RemoteFeeParseErr::UnexpectedEndAfterWithdraw),
		_ => return Err(RemoteFeeParseErr::InvalidFeeAsset),
	};
	let (fee_asset_id, fee_amount) = match fee_asset {
		Asset { id: asset_id, fun: Fungible(amount) } => (asset_id, *amount),
		_ => return Err(RemoteFeeParseErr::AssetResolutionFailed),
	};
	// The fee asset must be native Eth, represented by `Here`.
	if fee_asset_id.0 != Here.into() {
		return Err(RemoteFeeParseErr::InvalidFeeAsset);
	}
	if reserved_fee_asset_id.0 != Here.into() {
		return Err(RemoteFeeParseErr::InvalidFeeAsset);
	}
	if reserved_fee_amount < fee_amount {
		return Err(RemoteFeeParseErr::InvalidFeeAsset);
	}
	i += 1;
	Ok((i, fee_amount))
}

/// Optional `WithdrawAsset` (ENA) and/or `ReserveAssetDeposited` (PNA), in either order.
///
/// Returns the new instruction index and optional asset lists (same rules as
/// [`super::convert::XcmConverter::convert`]).
pub(crate) fn parse_optional_ena_pna<Call>(
	instructions: &[Instruction<Call>],
	mut i: usize,
) -> (usize, Option<&Assets>, Option<&Assets>) {
	let mut enas: Option<&Assets> = None;
	if let Some(Instruction::WithdrawAsset(assets)) = instructions.get(i) {
		enas = Some(assets);
		i += 1;
	}
	let mut pnas: Option<&Assets> = None;
	if let Some(Instruction::ReserveAssetDeposited(assets)) = instructions.get(i) {
		pnas = Some(assets);
		i += 1;
	}
	if enas.is_none() {
		if let Some(Instruction::WithdrawAsset(assets)) = instructions.get(i) {
			enas = Some(assets);
			i += 1;
		}
	}
	(i, enas, pnas)
}

/// Structural checks for an ENA [`Asset`] line (fungible; AccountKey20 or ether `[]`).
///
/// Does **not** enforce non-zero amount — [`super::convert::XcmConverter`] maps zero to
/// [`super::convert::XcmConverterError::ZeroAssetTransfer`]; [`super::shape`] checks `> 0`
/// separately.
pub(crate) fn ena_asset_matches_snowbridge_shape(
	asset: &Asset,
	ethereum_network: NetworkId,
) -> bool {
	let Asset { id: AssetId(loc), fun: Fungible(_) } = asset else {
		return false;
	};
	match loc.unpack() {
		// ERC20 token
		(0, [AccountKey20 { network, .. }])
			if network_matches_ethereum(network, ethereum_network) =>
		{
			true
		},
		// Native ETH
		(0, []) => true,
		_ => false,
	}
}

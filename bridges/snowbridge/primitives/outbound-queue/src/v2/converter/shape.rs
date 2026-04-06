// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Structural validation shared with [`super::convert::XcmConverter::convert`] for early rejection
//! (e.g. Bridge Hub Ethereum XCM simulation
//! [`ShouldExecute`](xcm_executor::traits::ShouldExecute)).

use frame_support::{ensure, traits::ProcessMessageError};
use xcm::prelude::*;

use super::syntax::{
	ena_asset_matches_snowbridge_shape, parse_optional_ena_pna, parse_remote_fee_section,
};

/// Returns `Ok(())` if `instructions` follows the Snowbridge v2 outbound syntax documented on
/// [`super::convert::XcmConverter::convert`].
///
/// This mirrors the converter’s instruction order and core checks (fees, reserves, origin,
/// beneficiary, optional `Transact` (or `ClearError` when `Transact` was replaced for export
/// simulation), `SetTopic`, no trailing instructions). PNA token registration
/// against [`snowbridge_core::TokenIdOf`] / asset-id mapping is still enforced only in
/// [`super::convert::XcmConverter::convert`].
pub fn snowbridge_v2_outbound_xcm_shape<Call>(
	instructions: &[Instruction<Call>],
	ethereum_network: NetworkId,
) -> Result<(), ProcessMessageError> {
	let mut i = 0;

	let (consumed, _) =
		parse_remote_fee_section(&instructions[i..]).map_err(|_| ProcessMessageError::BadFormat)?;
	i += consumed;

	let (next_i, enas, pnas) = parse_optional_ena_pna(instructions, i);
	i = next_i;

	let _origin_location = match instructions.get(i) {
		Some(Instruction::AliasOrigin(origin)) => origin,
		_ => return Err(ProcessMessageError::BadFormat),
	};
	// `AgentIdOf::convert_location` is enforced in [`super::convert::XcmConverter::convert`].
	i += 1;

	let (_deposit_filter, _beneficiary) = match instructions.get(i) {
		Some(Instruction::DepositAsset { assets, beneficiary }) => (assets, beneficiary),
		_ => return Err(ProcessMessageError::BadFormat),
	};
	i += 1;

	let mut has_transfer_commands = false;
	if let Some(assets) = enas {
		ensure!(assets.len() > 0, ProcessMessageError::BadFormat);
		for ena in assets.inner() {
			let Asset { fun: Fungible(amount), .. } = ena else {
				return Err(ProcessMessageError::BadFormat);
			};
			ensure!(*amount > 0, ProcessMessageError::BadFormat);
			if !ena_asset_matches_snowbridge_shape(ena, ethereum_network) {
				return Err(ProcessMessageError::BadFormat);
			}
			// `deposit_filter.matches` is validated in [`super::convert::XcmConverter::convert`];
			// wildcards and `AllCounted` can be subtle here, so we only enforce ENA shape above.
			has_transfer_commands = true;
		}
	}
	if let Some(assets) = pnas {
		ensure!(assets.len() > 0, ProcessMessageError::BadFormat);
		for pna in assets.inner() {
			let Asset { id: _, fun: Fungible(amount) } = pna else {
				return Err(ProcessMessageError::BadFormat);
			};
			ensure!(*amount > 0, ProcessMessageError::BadFormat);
			has_transfer_commands = true;
		}
	}

	// `ExecuteBeforeSnowbridgeV2BlobExport` replaces `Transact` with `ClearError` for dry-run
	// (`neutralize_eth_export_transacts_in_xcm_runtime`); accept both here.
	let mut has_transact = false;
	match instructions.get(i) {
		Some(Instruction::Transact { .. }) | Some(Instruction::ClearError) => {
			has_transact = true;
			i += 1;
		},
		_ => {},
	}

	ensure!(has_transfer_commands || has_transact, ProcessMessageError::BadFormat);

	// SetTopic is the last instruction required for tracing the message all the way along.
	match instructions.get(i) {
		Some(Instruction::SetTopic(_)) => {},
		_ => return Err(ProcessMessageError::BadFormat),
	}
	i += 1;

	if i != instructions.len() {
		return Err(ProcessMessageError::BadFormat);
	}

	Ok(())
}

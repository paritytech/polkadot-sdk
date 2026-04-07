// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Structural validation shared with [`super::convert::XcmConverter::convert`] for early rejection
//! (e.g. Bridge Hub Ethereum XCM simulation in
//! [`crate::v2::simulation::EthereumExportSimulationBarrier`]).

use frame_support::{ensure, traits::ProcessMessageError};
use snowbridge_core::AgentIdOf;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;

use super::syntax::{
	ena_asset_matches_snowbridge_shape, parse_optional_ena_pna, parse_remote_fee_section,
};

/// Top-level instructions that are not allowed in Snowbridge v2 outbound export blobs.
///
/// The v2 shape is a single flat instruction list; it must not contain wrapper/container
/// instructions that introduce nested XCM programs (e.g. error handlers, appendices, reserve
/// transfers) or mutate/assert on the Origin register outside the single allowed
/// [`Instruction::AliasOrigin`] slot.
fn disallowed_snowbridge_v2_top_level_instruction<Call>(inst: &Instruction<Call>) -> bool {
	matches!(
		inst,
		Instruction::ClearOrigin |
			Instruction::DescendOrigin(_) |
			Instruction::UniversalOrigin(_) |
			Instruction::ExpectOrigin(_) |
			Instruction::SetErrorHandler(_) |
			Instruction::SetAppendix(_) |
			Instruction::TransferReserveAsset { .. } |
			Instruction::DepositReserveAsset { .. } |
			Instruction::InitiateReserveWithdraw { .. } |
			Instruction::InitiateTeleport { .. } |
			Instruction::ExportMessage { .. } |
			Instruction::InitiateTransfer { .. } |
			Instruction::UnpaidExecution { .. } |
			Instruction::ExecuteWithOrigin { .. }
	)
}

/// Returns `Ok(())` if `instructions` follows the Snowbridge v2 outbound syntax documented on
/// [`super::convert::XcmConverter::convert`].
///
/// This mirrors the converter’s instruction order and core checks (fees, reserves, origin,
/// beneficiary, optional `Transact` (or `ClearError` when `Transact` was replaced for export
/// simulation), `SetTopic`, no trailing instructions). The [`AliasOrigin`] target must be
/// convertible with [`AgentIdOf::convert_location`], same as
/// [`super::convert::XcmConverter::convert`]. At top level, only [`AliasOrigin`] may affect the
/// Origin register (no [`Instruction::DescendOrigin`], [`Instruction::ClearOrigin`], etc.).
pub fn snowbridge_v2_outbound_xcm_shape<Call>(
	instructions: &[Instruction<Call>],
	ethereum_network: NetworkId,
) -> Result<(), ProcessMessageError> {
	for inst in instructions.iter() {
		if disallowed_snowbridge_v2_top_level_instruction(inst) {
			return Err(ProcessMessageError::BadFormat);
		}
	}

	let mut i = 0;

	let (consumed, _) =
		parse_remote_fee_section(&instructions[i..]).map_err(|_| ProcessMessageError::BadFormat)?;
	i += consumed;

	let (next_i, enas, pnas) = parse_optional_ena_pna(instructions, i);
	i = next_i;

	let origin_location = match instructions.get(i) {
		Some(Instruction::AliasOrigin(origin)) => origin,
		_ => return Err(ProcessMessageError::BadFormat),
	};
	ensure!(AgentIdOf::convert_location(origin_location).is_some(), ProcessMessageError::BadFormat);
	i += 1;

	// In normal v2 blobs this must be `DepositAsset`. During Bridge Hub export simulation, we may
	// deliberately short-circuit with `Trap(_)` (e.g. when the original blob contains an invalid or
	// malformed contract call) so the executor traps the holding register via `AssetTrap`.
	match instructions.get(i) {
		Some(Instruction::DepositAsset { .. }) => {
			i += 1;
		},
		Some(Instruction::Trap(_)) => return Ok(()),
		_ => return Err(ProcessMessageError::BadFormat),
	};

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

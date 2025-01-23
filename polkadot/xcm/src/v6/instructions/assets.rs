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

//! Asset related instructions.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use alloc::vec::Vec;

use crate::v6::{Asset, AssetFilter, AssetTransferFilter, Assets, Location, Xcm};

/// Withdraw asset(s) (`assets`) from the ownership of `origin` and place them into the Holding
/// Register.
///
/// - `assets`: The asset(s) to be withdrawn into holding.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct WithdrawAsset(pub Assets);

/// Asset(s) (`assets`) have been received into the ownership of this system on the `origin`
/// system and equivalent derivatives should be placed into the Holding Register.
///
/// - `assets`: The asset(s) that are minted into holding.
///
/// Safety: `origin` must be trusted to have received and be storing `assets` such that they
/// may later be withdrawn should this system send a corresponding message.
///
/// Kind: *Trusted Indication*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ReserveAssetDeposited(pub Assets);

/// Asset(s) (`assets`) have been destroyed on the `origin` system and equivalent assets should
/// be created and placed into the Holding Register.
///
/// - `assets`: The asset(s) that are minted into the Holding Register.
///
/// Safety: `origin` must be trusted to have irrevocably destroyed the corresponding `assets`
/// prior as a consequence of sending this message.
///
/// Kind: *Trusted Indication*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ReceiveTeleportedAsset(pub Assets);

/// Withdraw asset(s) (`assets`) from the ownership of `origin` and place equivalent assets
/// under the ownership of `beneficiary`.
///
/// - `assets`: The asset(s) to be withdrawn.
/// - `beneficiary`: The new owner for the assets.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct TransferAsset {
	pub assets: Assets,
	pub beneficiary: Location,
}

/// Withdraw asset(s) (`assets`) from the ownership of `origin` and place equivalent assets
/// under the ownership of `dest` within this consensus system (i.e. its sovereign account).
///
/// Send an onward XCM message to `dest` of `ReserveAssetDeposited` with the given
/// `xcm`.
///
/// - `assets`: The asset(s) to be withdrawn.
/// - `dest`: The location whose sovereign account will own the assets and thus the effective
///   beneficiary for the assets and the notification target for the reserve asset deposit
///   message.
/// - `xcm`: The instructions that should follow the `ReserveAssetDeposited` instruction, which
///   is sent onwards to `dest`.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct TransferReserveAsset {
	pub assets: Assets,
	pub dest: Location,
	pub xcm: Xcm<()>,
}

/// Remove the asset(s) (`assets`) from the Holding Register and place equivalent assets under
/// the ownership of `beneficiary` within this consensus system.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `beneficiary`: The new owner for the assets.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct DepositAsset {
	pub assets: AssetFilter,
	pub beneficiary: Location,
}

/// Remove the asset(s) (`assets`) from the Holding Register and place equivalent assets under
/// the ownership of `dest` within this consensus system (i.e. deposit them into its sovereign
/// account).
///
/// Send an onward XCM message to `dest` of `ReserveAssetDeposited` with the given `effects`.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `dest`: The location whose sovereign account will own the assets and thus the effective
///   beneficiary for the assets and the notification target for the reserve asset deposit
///   message.
/// - `xcm`: The orders that should follow the `ReserveAssetDeposited` instruction which is
///   sent onwards to `dest`.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct DepositReserveAsset {
	pub assets: AssetFilter,
	pub dest: Location,
	pub xcm: Xcm<()>,
}

/// Remove the asset(s) (`want`) from the Holding Register and replace them with alternative
/// assets.
///
/// The minimum amount of assets to be received into the Holding Register for the order not to
/// fail may be stated.
///
/// - `give`: The maximum amount of assets to remove from holding.
/// - `want`: The minimum amount of assets which `give` should be exchanged for.
/// - `maximal`: If `true`, then prefer to give as much as possible up to the limit of `give`
///   and receive accordingly more. If `false`, then prefer to give as little as possible in
///   order to receive as little as possible while receiving at least `want`.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ExchangeAsset {
	pub give: AssetFilter,
	pub want: Assets,
	pub maximal: bool,
}

/// Remove the asset(s) (`assets`) from holding and send a `WithdrawAsset` XCM message to a
/// reserve location.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `reserve`: A valid location that acts as a reserve for all asset(s) in `assets`. The
///   sovereign account of this consensus system *on the reserve location* will have
///   appropriate assets withdrawn and `effects` will be executed on them. There will typically
///   be only one valid location on any given asset/chain combination.
/// - `xcm`: The instructions to execute on the assets once withdrawn *on the reserve
///   location*.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct InitiateReserveWithdraw {
	pub assets: AssetFilter,
	pub reserve: Location,
	pub xcm: Xcm<()>,
}

/// Remove the asset(s) (`assets`) from holding and send a `ReceiveTeleportedAsset` XCM message
/// to a `dest` location.
///
/// - `assets`: The asset(s) to remove from holding.
/// - `dest`: A valid location that respects teleports coming from this location.
/// - `xcm`: The instructions to execute on the assets once arrived *on the destination
///   location*.
///
/// NOTE: The `dest` location *MUST* respect this origin as a valid teleportation origin for
/// all `assets`. If it does not, then the assets may be lost.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct InitiateTeleport {
	pub assets: AssetFilter,
	pub dest: Location,
	pub xcm: Xcm<()>,
}

/// Create some assets which are being held on behalf of the origin.
///
/// - `assets`: The assets which are to be claimed. This must match exactly with the assets
///   claimable by the origin of the ticket.
/// - `ticket`: The ticket of the asset; this is an abstract identifier to help locate the
///   asset.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ClaimAsset {
	pub assets: Assets,
	pub ticket: Location,
}

/// Reduce Holding by up to the given assets.
///
/// Holding is reduced by as much as possible up to the assets in the parameter. It is not an
/// error if the Holding does not contain the assets (to make this an error, use `ExpectAsset`
/// prior).
///
/// Kind: *Command*
///
/// Errors: *Infallible*
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct BurnAsset(pub Assets);

/// Lock the locally held asset and prevent further transfer or withdrawal.
///
/// This restriction may be removed by the `UnlockAsset` instruction being called with an
/// Origin of `unlocker` and a `target` equal to the current `Origin`.
///
/// If the locking is successful, then a `NoteUnlockable` instruction is sent to `unlocker`.
///
/// - `asset`: The asset(s) which should be locked.
/// - `unlocker`: The value which the Origin must be for a corresponding `UnlockAsset`
///   instruction to work.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct LockAsset {
	pub asset: Asset,
	pub unlocker: Location,
}

/// Remove the lock over `asset` on this chain and (if nothing else is preventing it) allow the
/// asset to be transferred.
///
/// - `asset`: The asset to be unlocked.
/// - `target`: The owner of the asset on the local chain.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct UnlockAsset {
	pub asset: Asset,
	pub target: Location,
}

/// Asset (`asset`) has been locked on the `origin` system and may not be transferred. It may
/// only be unlocked with the receipt of the `UnlockAsset` instruction from this chain.
///
/// - `asset`: The asset(s) which are now unlockable from this origin.
/// - `owner`: The owner of the asset on the chain in which it was locked. This may be a
///   location specific to the origin network.
///
/// Safety: `origin` must be trusted to have locked the corresponding `asset`
/// prior as a consequence of sending this message.
///
/// Kind: *Trusted Indication*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct NoteUnlockable {
	pub asset: Asset,
	pub owner: Location,
}

/// Send an `UnlockAsset` instruction to the `locker` for the given `asset`.
///
/// This may fail if the local system is making use of the fact that the asset is locked or,
/// of course, if there is no record that the asset actually is locked.
///
/// - `asset`: The asset(s) to be unlocked.
/// - `locker`: The location from which a previous `NoteUnlockable` was sent and to which an
///   `UnlockAsset` should be sent.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct RequestUnlock {
	pub asset: Asset,
	pub locker: Location,
}

/// Initiates cross-chain transfer as follows:
///
/// Assets in the holding register are matched using the given list of `AssetTransferFilter`s,
/// they are then transferred based on their specified transfer type:
///
/// - teleport: burn local assets and append a `ReceiveTeleportedAsset` XCM instruction to the
///   XCM program to be sent onward to the `destination` location,
///
/// - reserve deposit: place assets under the ownership of `destination` within this consensus
///   system (i.e. its sovereign account), and append a `ReserveAssetDeposited` XCM instruction
///   to the XCM program to be sent onward to the `destination` location,
///
/// - reserve withdraw: burn local assets and append a `WithdrawAsset` XCM instruction to the
///   XCM program to be sent onward to the `destination` location,
///
/// The onward XCM is then appended a `ClearOrigin` to allow safe execution of any following
/// custom XCM instructions provided in `remote_xcm`.
///
/// The onward XCM also contains either a `PayFees` or `UnpaidExecution` instruction based
/// on the presence of the `remote_fees` parameter (see below).
///
/// If an XCM program requires going through multiple hops, it can compose this instruction to
/// be used at every chain along the path, describing that specific leg of the flow.
///
/// Parameters:
/// - `destination`: The location of the program next hop.
/// - `remote_fees`: If set to `Some(asset_xfer_filter)`, the single asset matching
///   `asset_xfer_filter` in the holding register will be transferred first in the remote XCM
///   program, followed by a `PayFees(fee)`, then rest of transfers follow. This guarantees
///   `remote_xcm` will successfully pass a `AllowTopLevelPaidExecutionFrom` barrier. If set to
///   `None`, a `UnpaidExecution` instruction is appended instead. Please note that these
///   assets are **reserved** for fees, they are sent to the fees register rather than holding.
///   Best practice is to only add here enough to cover fees, and transfer the rest through the
///   `assets` parameter.
/// - `preserve_origin`: Specifies whether the original origin should be preserved or cleared,
///   using the instructions `AliasOrigin` or `ClearOrigin` respectively.
/// - `assets`: List of asset filters matched against existing assets in holding. These are
///   transferred over to `destination` using the specified transfer type, and deposited to
///   holding on `destination`.
/// - `remote_xcm`: Custom instructions that will be executed on the `destination` chain. Note
///   that these instructions will be executed after a `ClearOrigin` so their origin will be
///   `None`.
///
/// Safety: No concerns.
///
/// Kind: *Command*
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct InitiateTransfer {
	pub destination: Location,
	pub remote_fees: Option<AssetTransferFilter>,
	pub preserve_origin: bool,
	pub assets: Vec<AssetTransferFilter>,
	pub remote_xcm: Xcm<()>,
}

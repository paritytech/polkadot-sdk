// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

/// Declares a runtime-specific `BridgeRejectObsoleteParachainHeader` signed extension.
///
/// ## Example
///
/// ```nocompile
/// pallet_bridge_grandpa::declare_bridge_reject_obsolete_parachain_header!{
///     Runtime,
///     Call::BridgeRialtoParachains => RialtoGrandpaInstance,
///     Call::BridgeWestendParachains => WestendGrandpaInstance,
/// }
/// ```
///
/// The goal of this extension is to avoid "mining" transactions that provide
/// outdated bridged parachain heads. Without that extension, even honest relayers
/// may lose their funds if there are multiple relays running and submitting the
/// same information.
///
/// This extension only works with transactions that are updating single parachain
/// head. We can't use unbounded validation - it may take too long and either break
/// block production, or "eat" significant portion of block production time literally
/// for nothing. In addition, the single-parachain-head-per-transaction is how the
/// pallet will be used in our environment.
#[macro_export]
macro_rules! declare_bridge_reject_obsolete_parachain_header {
	($runtime:ident, $($call:path => $instance:ty),*) => {
		/// Transaction-with-obsolete-bridged-parachain-header check that will reject transaction if
		/// it submits obsolete bridged parachain header.
		#[derive(Clone, codec::Decode, codec::Encode, Eq, PartialEq, frame_support::RuntimeDebug, scale_info::TypeInfo)]
		pub struct BridgeRejectObsoleteParachainHeader;

		impl sp_runtime::traits::SignedExtension for BridgeRejectObsoleteParachainHeader {
			const IDENTIFIER: &'static str = "BridgeRejectObsoleteParachainHeader";
			type AccountId = <$runtime as frame_system::Config>::AccountId;
			type Call = <$runtime as frame_system::Config>::Call;
			type AdditionalSigned = ();
			type Pre = ();

			fn additional_signed(&self) -> sp_std::result::Result<
				(),
				sp_runtime::transaction_validity::TransactionValidityError,
			> {
				Ok(())
			}

			fn validate(
				&self,
				_who: &Self::AccountId,
				call: &Self::Call,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_len: usize,
			) -> sp_runtime::transaction_validity::TransactionValidity {
				match *call {
					$(
						$call($crate::Call::<$runtime, $instance>::submit_parachain_heads {
							ref at_relay_block,
							ref parachains,
							..
						}) if parachains.len() == 1 => {
							let parachain = parachains.get(0).expect("verified by match condition; qed");

							let bundled_relay_block_number = at_relay_block.0;

							let best_parachain_head = $crate::BestParaHeads::<$runtime, $instance>::get(parachain);
							match best_parachain_head {
								Some(best_parachain_head) if best_parachain_head.at_relay_block_number
									>= bundled_relay_block_number =>
										sp_runtime::transaction_validity::InvalidTransaction::Stale.into(),
								_ => Ok(sp_runtime::transaction_validity::ValidTransaction::default()),
							}
						},
					)*
					_ => Ok(sp_runtime::transaction_validity::ValidTransaction::default()),
				}
			}

			fn pre_dispatch(
				self,
				who: &Self::AccountId,
				call: &Self::Call,
				info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				len: usize,
			) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
				self.validate(who, call, info, len).map(drop)
			}

			fn post_dispatch(
				_maybe_pre: Option<Self::Pre>,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_post_info: &sp_runtime::traits::PostDispatchInfoOf<Self::Call>,
				_len: usize,
				_result: &sp_runtime::DispatchResult,
			) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
				Ok(())
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use crate::{
		mock::{run_test, Call, TestRuntime},
		BestParaHead, BestParaHeads, RelayBlockNumber,
	};
	use bp_polkadot_core::parachains::{ParaHeadsProof, ParaId};
	use frame_support::weights::{DispatchClass, DispatchInfo, Pays};
	use sp_runtime::traits::SignedExtension;

	declare_bridge_reject_obsolete_parachain_header! {
		TestRuntime,
		Call::Parachains => ()
	}

	fn validate_submit_parachain_heads(num: RelayBlockNumber, parachains: Vec<ParaId>) -> bool {
		BridgeRejectObsoleteParachainHeader
			.validate(
				&42,
				&Call::Parachains(crate::Call::<TestRuntime, ()>::submit_parachain_heads {
					at_relay_block: (num, Default::default()),
					parachains,
					parachain_heads_proof: ParaHeadsProof(Vec::new()),
				}),
				&DispatchInfo { weight: 0, class: DispatchClass::Operational, pays_fee: Pays::Yes },
				0,
			)
			.is_ok()
	}

	fn sync_to_relay_header_10() {
		BestParaHeads::<TestRuntime, ()>::insert(
			ParaId(1),
			BestParaHead {
				at_relay_block_number: 10,
				head_hash: Default::default(),
				next_imported_hash_position: 0,
			},
		);
	}

	#[test]
	fn extension_rejects_obsolete_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#5 => tx is
			// rejected
			sync_to_relay_header_10();
			assert!(!validate_submit_parachain_heads(5, vec![ParaId(1)]));
		});
	}

	#[test]
	fn extension_rejects_same_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#10 => tx is
			// rejected
			sync_to_relay_header_10();
			assert!(!validate_submit_parachain_heads(10, vec![ParaId(1)]));
		});
	}

	#[test]
	fn extension_accepts_new_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#15 => tx is
			// accepted
			sync_to_relay_header_10();
			assert!(validate_submit_parachain_heads(15, vec![ParaId(1)]));
		});
	}

	#[test]
	fn extension_accepts_if_more_than_one_parachain_is_submitted() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#5, but another
			// parachain head is also supplied => tx is accepted
			sync_to_relay_header_10();
			assert!(validate_submit_parachain_heads(5, vec![ParaId(1), ParaId(2)]));
		});
	}
}

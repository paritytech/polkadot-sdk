#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_consensus_babe::AuthorityId;
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair as PairT;
use sp_price_oracle::{Nudge, SignedNudge};
use sp_runtime::FixedU128;

use crate::*;

fn make_signed_nudge(authority_index: u32, slot: u64, nudge: Nudge) -> SignedNudge {
	let pair = sp_core::sr25519::Pair::from_seed(&[authority_index as u8; 32]);
	let slot = Slot::from(slot);
	let payload = SignedNudge::signing_payload(&nudge, slot);
	let sig = pair.sign(&payload);
	SignedNudge {
		nudge,
		slot,
		authority_index,
		signature: sp_consensus_babe::AuthoritySignature::from(sig),
	}
}

fn setup_authorities<T: Config>(n: u32) -> Vec<AuthorityId> {
	let authorities: Vec<AuthorityId> = (0..n)
		.map(|i| {
			let pair = sp_core::sr25519::Pair::from_seed(&[i as u8; 32]);
			AuthorityId::from(pair.public())
		})
		.collect();
	authorities
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn submit_nudges(n: Linear<1, 300>) {
		let _authorities = setup_authorities::<T>(n);
		let slot = 100u64;
		let nudges: Vec<SignedNudge> =
			(0..n).map(|i| make_signed_nudge(i, slot, Nudge::Up)).collect();

		#[extrinsic_call]
		_(RawOrigin::None, nudges);
	}
}

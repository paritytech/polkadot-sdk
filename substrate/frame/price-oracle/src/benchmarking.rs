#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_consensus_babe::AuthorityId;
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair as PairT;
use sp_price_oracle::{Nudge, PairConfig, PairId, SignedNudge};
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

fn default_cfg() -> PairConfig {
	PairConfig {
		min_nudges: 0,
		nudge_validity: 1_000,
		inherent_mandatory: false,
		invalid_inherent_panics: false,
		epsilon: FixedU128::from_rational(1, 100),
	}
}

fn register_pair_inline<T: Config>(pair_id: PairId) {
	pallet::Pairs::<T>::insert(pair_id, default_cfg());
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn submit_nudges(p: Linear<1, 8>, n: Linear<1, 300>) {
		let _authorities = setup_authorities::<T>(n);
		let slot = 100u64;

		let pair_nudges: Vec<(PairId, Vec<SignedNudge>)> = (0..p as u8)
			.map(|pid| {
				register_pair_inline::<T>(pid);
				let nudges: Vec<SignedNudge> =
					(0..n).map(|i| make_signed_nudge(i, slot, Nudge::Up)).collect();
				(pid, nudges)
			})
			.collect();

		#[extrinsic_call]
		_(RawOrigin::None, pair_nudges);
	}

	#[benchmark]
	fn register_pair() {
		#[extrinsic_call]
		_(RawOrigin::Root, 0u8, default_cfg(), FixedU128::zero());
	}

	#[benchmark]
	fn update_pair_config() {
		register_pair_inline::<T>(0u8);
		#[extrinsic_call]
		_(RawOrigin::Root, 0u8, default_cfg());
	}

	#[benchmark]
	fn remove_pair() {
		register_pair_inline::<T>(0u8);
		#[extrinsic_call]
		_(RawOrigin::Root, 0u8);
	}

	#[benchmark]
	fn set_active_endpoints(e: Linear<0, 20>) {
		register_pair_inline::<T>(0u8);
		let endpoints: Vec<(u8, Vec<u8>)> = (0..e)
			.map(|_| (u8::from(ParsingMethod::Binance), b"https://x".to_vec()))
			.collect();
		#[extrinsic_call]
		_(RawOrigin::Root, 0u8, endpoints);
	}
}

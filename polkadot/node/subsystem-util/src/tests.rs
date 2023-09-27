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

#![cfg(test)]

use super::*;
use executor::block_on;
use futures::{channel::mpsc, executor, FutureExt, SinkExt, StreamExt};
use polkadot_primitives_test_helpers::AlwaysZeroRng;
use std::{
	collections::HashSet,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	},
	time::Duration,
};

#[test]
fn tick_tack_metronome() {
	let n = Arc::new(AtomicUsize::default());

	let (tick, mut block) = mpsc::unbounded();

	let metronome = {
		let n = n.clone();
		let stream = Metronome::new(Duration::from_millis(137_u64));
		stream
			.for_each(move |_res| {
				let _ = n.fetch_add(1, Ordering::Relaxed);
				let mut tick = tick.clone();
				async move {
					tick.send(()).await.expect("Test helper channel works. qed");
				}
			})
			.fuse()
	};

	let f2 = async move {
		block.next().await;
		assert_eq!(n.load(Ordering::Relaxed), 1_usize);
		block.next().await;
		assert_eq!(n.load(Ordering::Relaxed), 2_usize);
		block.next().await;
		assert_eq!(n.load(Ordering::Relaxed), 3_usize);
		block.next().await;
		assert_eq!(n.load(Ordering::Relaxed), 4_usize);
	}
	.fuse();

	futures::pin_mut!(f2);
	futures::pin_mut!(metronome);

	block_on(async move {
		// futures::join!(metronome, f2)
		futures::select!(
			_ = metronome => unreachable!("Metronome never stops. qed"),
			_ = f2 => (),
		)
	});
}

#[test]
fn subset_generation_check() {
	let mut values = (0_u8..=25).collect::<Vec<_>>();
	// 12 even numbers exist
	choose_random_subset::<u8, _>(|v| v & 0x01 == 0, &mut values, 12);
	values.sort();
	for (idx, v) in dbg!(values).into_iter().enumerate() {
		assert_eq!(v as usize, idx * 2);
	}
}

#[test]
fn subset_predefined_generation_check() {
	let mut values = (0_u8..=25).collect::<Vec<_>>();
	choose_random_subset_with_rng::<u8, _, _>(|_| false, &mut values, &mut AlwaysZeroRng, 12);
	assert_eq!(values.len(), 12);
	for (idx, v) in dbg!(values).into_iter().enumerate() {
		// Since shuffle actually shuffles the indexes from 1..len, then
		// our PRG that returns zeroes will shuffle 0 and 1, 1 and 2, ... len-2 and len-1
		assert_eq!(v as usize, idx + 1);
	}
}

#[test]
fn test_availability_chunk_indices() {
	let block_number = 89;
	let n_validators = 11;
	let babe_randomness = [12u8; 32];
	let n_cores = 4;

	let client_features = Some(ClientFeatures::AVAILABILITY_CHUNK_SHUFFLING);
	let (shuffle, core_start_pos) = availability_chunk_indices(
		client_features,
		block_number,
		babe_randomness,
		n_validators,
		n_cores,
	);
	// assert_eq!(shuffle, (0..n_validators).map(|i| ValidatorIndex(i as u32)).collect::<Vec<_>>());
	// assert_eq!(core_start_pos, repeat(ValidatorIndex(0)).take(n_cores).collect::<Vec<_>>());

	for index in 0..n_cores {
		for validator in 0..n_validators {
			let chunk_index = availability_chunk_index(
				client_features,
				block_number,
				babe_randomness,
				n_validators,
				CoreIndex(index as u32),
				n_cores,
				ValidatorIndex(validator as u32),
			);

			assert_eq!(
				&chunk_index,
				(shuffle
					.iter()
					.cycle()
					.skip(core_start_pos[index].0 as usize)
					.take(n_validators)
					.collect::<Vec<_>>())[validator]
			);
		}
	}

	// let shuffle =
	// 	availability_chunk_indices(Some(ClientFeatures::empty()), block_number, n_validators);
	// assert_eq!(shuffle, (0..n_validators).map(|i| ValidatorIndex(i as u32)).collect::<Vec<_>>());

	// let shuffle =
	// 	availability_chunk_indices(ClientFeatures::from_bits(0b10), block_number, n_validators);
	// assert_eq!(shuffle, (0..n_validators).map(|i| ValidatorIndex(i as u32)).collect::<Vec<_>>());

	// let shuffle = availability_chunk_indices(
	// 	Some(ClientFeatures::AVAILABILITY_CHUNK_SHUFFLING),
	// 	block_number,
	// 	n_validators,
	// );
	// assert_ne!(shuffle, (0..n_validators).map(|i| ValidatorIndex(i as u32)).collect::<Vec<_>>());
	// assert_eq!(shuffle.len(), n_validators);
	// assert_eq!(shuffle.iter().collect::<HashSet<_>>().len(), n_validators);
}

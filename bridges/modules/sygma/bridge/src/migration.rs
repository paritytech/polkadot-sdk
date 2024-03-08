// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

#[allow(unused_imports)]
use super::*;

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
use frame_support::traits::{Get, OnRuntimeUpgrade, StorageVersion};
use log;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;
use sygma_traits::MpcAddress;

const EXPECTED_STORAGE_VERSION: StorageVersion = StorageVersion::new(0);
#[cfg(feature = "try-runtime")]
const FINAL_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
const MPC_ADDR: &str = "B01137123EF02fAeF251a39108c6ef513AAaC485";

pub struct FixMpcAddress<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for FixMpcAddress<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if StorageVersion::get::<Pallet<T>>() == EXPECTED_STORAGE_VERSION {
			log::info!("Start sygma bridge migration");

			let mut slice: [u8; 20] = [0; 20];
			slice.copy_from_slice(&hex::decode(MPC_ADDR).unwrap()[..20]);
			MpcAddr::<T>::kill();
			MpcAddr::<T>::set(MpcAddress(slice));

			// Set new storage version to 1
			StorageVersion::new(1).put::<Pallet<T>>();

			log::info!("Sygma bridge migration done👏");

			// kill + set + put
			T::DbWeight::get().writes(3)
		} else {
			T::DbWeight::get().reads(1)
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		ensure!(
			StorageVersion::get::<Pallet<T>>() == EXPECTED_STORAGE_VERSION,
			"Incorrect Sygma bridge storage version in pre migrate"
		);

		log::info!("Sygma bridge pre migration check passed👏");

		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
		ensure!(
			StorageVersion::get::<Pallet<T>>() == FINAL_STORAGE_VERSION,
			"Incorrect Sygma bridge storage version in post migrate"
		);

		let mut slice: [u8; 20] = [0; 20];
		slice.copy_from_slice(&hex::decode(MPC_ADDR).unwrap()[..20]);
		ensure!(MpcAddr::<T>::get() == MpcAddress(slice), "Unexpected MPC address in post migrate");

		log::info!("Sygma bridge post migration check passed👏");

		Ok(())
	}
}

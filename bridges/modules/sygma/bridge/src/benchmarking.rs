// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

//! Sygma bridge pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]
use super::*;
use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::{crypto::ecdsa::ECDSAExt, traits::Currency};
use frame_system::RawOrigin as SystemOrigin;
use primitive_types::U256;
use sp_runtime::AccountId32;
use sp_std::{borrow::Borrow, prelude::*};

use sygma_fee_handler_router::FeeHandlerType;
use sygma_traits::{ChainID, DomainID, MpcAddress, ResourceId};

use crate::Pallet as SygmaBridge;
use sygma_basic_feehandler::Pallet as BasicFeeHandler;
use sygma_fee_handler_router::Pallet as FeeHandlerRouter;

use pallet_balances::Pallet as Balances;
use sp_std::{boxed::Box, vec};
use xcm::latest::prelude::*;

pub fn slice_to_generalkey(key: &[u8]) -> Junction {
	let len = key.len();
	assert!(len <= 32);
	GeneralKey {
		length: len as u8,
		data: {
			let mut data = [0u8; 32];
			data[..len].copy_from_slice(key);
			data
		},
	}
}

#[benchmarks(
    where
		T: pallet_balances::Config,
		T: sygma_basic_feehandler::Config,
		T: sygma_fee_handler_router::Config,
        <T as frame_system::Config>::AccountId: From<[u8; 32]> + Into<[u8; 32]> + From<AccountId32>,
        <T as pallet_balances::Config>::Balance: From<u128>,
        sp_runtime::AccountId32: Borrow<<T as frame_system::Config>::AccountId>,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn pause_bridge() {
		let dest_domain_id: DomainID = 0;
		let dest_chain_id: ChainID = U256::from(1);
		SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), dest_domain_id, dest_chain_id)
			.unwrap();

		#[extrinsic_call]
		pause_bridge(SystemOrigin::Root, dest_domain_id);

		assert!(IsPaused::<T>::get(dest_domain_id));
	}

	#[benchmark]
	fn unpause_bridge() {
		let dest_domain_id: DomainID = 0;
		let dest_chain_id: ChainID = U256::from(1);
		SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), dest_domain_id, dest_chain_id)
			.unwrap();
		SygmaBridge::<T>::pause_bridge(SystemOrigin::Root.into(), dest_domain_id).unwrap();

		#[extrinsic_call]
		unpause_bridge(SystemOrigin::Root, dest_domain_id);

		assert!(!IsPaused::<T>::get(dest_domain_id));
	}

	#[benchmark]
	fn set_mpc_address() {
		let test_mpc_addr: MpcAddress = MpcAddress([1u8; 20]);

		#[extrinsic_call]
		set_mpc_address(SystemOrigin::Root, test_mpc_addr);

		assert_eq!(MpcAddr::<T>::get(), test_mpc_addr);
	}

	#[benchmark]
	fn register_domain() {
		let dest_domain_id: DomainID = 0;
		let dest_chain_id: ChainID = U256::from(1);

		#[extrinsic_call]
		register_domain(SystemOrigin::Root, dest_domain_id, dest_chain_id);

		assert!(DestDomainIds::<T>::get(&dest_domain_id));
	}

	#[benchmark]
	fn unregister_domain() {
		let dest_domain_id: DomainID = 0;
		let dest_chain_id: ChainID = U256::from(1);

		SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), dest_domain_id, dest_chain_id)
			.unwrap();

		#[extrinsic_call]
		unregister_domain(SystemOrigin::Root, dest_domain_id, dest_chain_id);

		assert!(!DestDomainIds::<T>::get(&dest_domain_id));
	}

	#[benchmark]
	fn deposit() {
		let treasury_account: AccountId32 = AccountId32::new([100u8; 32]);
		let bridge_account: AccountId32 = AccountId32::new([101u8; 32]);
		let native_location: MultiLocation = MultiLocation::here();

		let dest_domain_id: DomainID = 1;
		let dest_chain_id: ChainID = U256::from(1);
		let test_mpc_addr: MpcAddress = MpcAddress([1u8; 20]);
		let fee = 1_000_000_000_000u128; // 1 with 12 decimals
		let amount = 200_000_000_000_000u128; // 200 with 12 decimals
		let caller = whitelisted_caller::<AccountId32>();

		let _ = <Balances<T, _> as Currency<_>>::make_free_balance_be(
			&caller.clone().into(),
			(amount * 2).into(),
		);

		BasicFeeHandler::<T>::set_fee(
			SystemOrigin::Root.into(),
			dest_domain_id,
			Box::new(native_location.clone().into()),
			fee,
		)
		.unwrap();
		FeeHandlerRouter::<T>::set_fee_handler(
			SystemOrigin::Root.into(),
			dest_domain_id,
			Box::new(native_location.clone().into()),
			FeeHandlerType::BasicFeeHandler,
		)
		.unwrap();

		SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), dest_domain_id, dest_chain_id)
			.unwrap();
		SygmaBridge::<T>::set_mpc_address(SystemOrigin::Root.into(), test_mpc_addr).unwrap();

		#[extrinsic_call]
		deposit(
			SystemOrigin::Signed(caller.clone().into()),
			Box::new((Concrete(native_location), Fungible(amount)).into()),
			Box::new(MultiLocation {
				parents: 0,
				interior: X2(
					slice_to_generalkey(b"ethereum recipient"),
					slice_to_generalkey(&[dest_domain_id]),
				),
			}),
		);

		assert_eq!(Balances::<T, _>::free_balance(caller), amount.into());
		assert_eq!(Balances::<T, _>::free_balance(bridge_account), (amount - fee).into());
		assert_eq!(Balances::<T, _>::free_balance(treasury_account), fee.into());
	}

	#[benchmark]
	fn retry() {
		let dest_domain_id: DomainID = 1;
		let dest_chain_id: ChainID = U256::from(1);
		let test_mpc_addr: MpcAddress = MpcAddress([1u8; 20]);

		SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), dest_domain_id, dest_chain_id)
			.unwrap();
		SygmaBridge::<T>::set_mpc_address(SystemOrigin::Root.into(), test_mpc_addr).unwrap();

		#[extrinsic_call]
		retry(SystemOrigin::Root, 123, dest_domain_id);
	}

	#[benchmark]
	fn execute_proposal(n: Linear<1, 1_000>) {
		let caller = whitelisted_caller::<AccountId32>();
		let amount = 200_000_000_000_000u128;
		let dest_domain_id: DomainID = 1;
		let bridge_account: AccountId32 = AccountId32::new([101u8; 32]);
		let native_resourceid: ResourceId =
			hex_literal::hex!("0000000000000000000000000000000000000000000000000000000000000001");
		// set mpc address to generated keypair's address
		let key_type = sp_core::crypto::KeyTypeId(*b"code");
		let pub_key = sp_io::crypto::ecdsa_generate(key_type, None);

		// let (pair, _): (ecdsa::Pair, _) = Pair::generate();
		let test_mpc_addr: MpcAddress = MpcAddress(pub_key.to_eth_address().unwrap());
		SygmaBridge::<T>::set_mpc_address(SystemOrigin::Root.into(), test_mpc_addr).unwrap();

		let _ = <Balances<T, _> as Currency<_>>::make_free_balance_be(
			&bridge_account.clone().into(),
			(amount).into(),
		);
		assert_eq!(Balances::<T, _>::free_balance(bridge_account.clone()), (amount).into());

		// register domain
		SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), dest_domain_id, U256::from(1))
			.unwrap();

		// Generate proposals
		// amount is in 18 decimal 0.000200000000000000, will be convert to 12 decimal
		// 0.000200000000
		let native_transfer_proposal = Proposal {
			origin_domain_id: dest_domain_id,
			deposit_nonce: 1,
			resource_id: native_resourceid,
			data: SygmaBridge::<T>::create_deposit_data(
				amount,
				MultiLocation::new(
					0,
					X1(Junction::AccountId32 { network: None, id: caller.clone().into() }),
				)
				.encode(),
			),
		};

		let mut proposals = vec![];
		for _ in 0..n {
			proposals.push(native_transfer_proposal.clone());
		}

		let final_message: [u8; 32] =
			SygmaBridge::<T>::construct_ecdsa_signing_proposals_data(&proposals);
		// let proposals_with_valid_signature = pair.sign_prehashed(&final_message);
		let proposals_with_valid_signature =
			sp_io::crypto::ecdsa_sign_prehashed(key_type, &pub_key, &final_message)
				.expect("Generates signature");

		// Only the first proposal will  execute successfully, others will fail due to deposit nonce
		#[extrinsic_call]
		execute_proposal(SystemOrigin::Root, proposals, proposals_with_valid_signature.encode());

		// proposal amount is in 18 decimal 0.000200000000000000, will be convert to 12
		// decimal 0.000200000000(200000000) because native asset is defined in 12 decimal
		assert_eq!(Balances::<T, _>::free_balance(caller), 200000000.into());
		assert_eq!(Balances::<T, _>::free_balance(bridge_account), (amount - 200000000).into());
	}

	#[benchmark]
	fn pause_all_bridges() {
		let domain_size = 2;

		for i in 1..domain_size + 1 {
			SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), i, U256::from(i)).unwrap();
		}

		#[extrinsic_call]
		pause_all_bridges(SystemOrigin::Root);

		for i in 1..domain_size + 1 {
			assert!(IsPaused::<T>::get(i));
		}
	}

	#[benchmark]
	fn unpause_all_bridges() {
		let domain_size = 2;
		let test_mpc_addr: MpcAddress = MpcAddress([1u8; 20]);

		for i in 1..domain_size + 1 {
			SygmaBridge::<T>::register_domain(SystemOrigin::Root.into(), i, U256::from(i)).unwrap();
		}

		SygmaBridge::<T>::set_mpc_address(SystemOrigin::Root.into(), test_mpc_addr).unwrap();

		for i in 1..domain_size + 1 {
			SygmaBridge::<T>::pause_bridge(SystemOrigin::Root.into(), i).unwrap();
		}

		#[extrinsic_call]
		unpause_all_bridges(SystemOrigin::Root);

		for i in 1..domain_size + 1 {
			assert!(!IsPaused::<T>::get(i));
		}
	}
}

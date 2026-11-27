// This file is part of Cumulus.
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Build both authorizers with `cargo jam-build --out-dir <dir>`, then run
//! `AURA_BLOB_DIR=<absolute-dir> cargo test -p cumulus-aura-authorizer-utils --test blob --
//! --ignored`.

use cumulus_aura_authorizer_utils::{
	build_collator_proof, AuthConfig, AuthToken, ParaId, WorkPackageBody,
};
use jam_types::{
	Authorizer, Encode as JamEncode, FetchKind, IsAuthorizedParams, RefineContext, WorkItem,
};
use polkavm::{BackendKind, Engine, InterruptKind, Module, ProgramCounter, Reg};

fn execute(module: &Module, package: &[u8]) -> Option<Vec<u8>> {
	let mut vm = module.instantiate().expect("instantiate authorizer");
	let args = IsAuthorizedParams { core: 0 }.encode();
	let address = module.memory_map().aux_data_address();
	vm.set_accessible_aux_size(args.len() as u32).expect("map arguments");
	vm.write_memory(address, &args).expect("write arguments");
	vm.set_host_side_aux_write_protect(true).expect("protect arguments");
	vm.set_gas(jam_types::max_is_authorized_gas() as i64);
	vm.prepare_call_typed(ProgramCounter(0), (address, args.len() as u32));
	let params = jam_types::ProtocolParameters::get().encode();
	loop {
		match vm.run().expect("run interpreter") {
			InterruptKind::Finished => {
				return Some(
					vm.read_memory(vm.reg(Reg::A0) as u32, vm.reg(Reg::A1) as u32).expect("trace"),
				);
			},
			InterruptKind::Trap => return None,
			InterruptKind::Ecalli(1) => {
				// JAM grow_heap takes an absolute page count and returns the new heap top in pages.
				let page = module.memory_map().page_size();
				let top = vm.sbrk(0).expect("read heap top").expect("mapped heap");
				let requested = u32::try_from(vm.reg(Reg::A0)).expect("page count") * page;
				let top =
					vm.sbrk(requested.saturating_sub(top)).expect("grow heap").expect("heap fits");
				vm.set_reg(Reg::A0, u64::from(top.div_ceil(page)));
			},
			InterruptKind::Ecalli(2) => {
				let bytes = match FetchKind::try_from(vm.reg(Reg::A3)) {
					Ok(FetchKind::ProtocolParameters) => &params[..],
					Ok(FetchKind::WorkPackage) => package,
					kind => panic!("unexpected fetch: {kind:?}"),
				};
				let offset = bytes.len().min(vm.reg(Reg::A1) as usize);
				let len = (bytes.len() - offset).min(vm.reg(Reg::A2) as usize);
				if len != 0 {
					vm.write_memory(vm.reg(Reg::A0) as u32, &bytes[offset..offset + len])
						.expect("fetch destination");
				}
				vm.set_reg(Reg::A0, bytes.len() as u64);
			},
			other => panic!("unexpected interpreter exit: {other:?}"),
		}
	}
}

fn check(scheme: &str, key: [u8; 32], sign: impl FnOnce(&[u8]) -> [u8; 64]) {
	let dir =
		std::path::PathBuf::from(std::env::var_os("AURA_BLOB_DIR").expect("set AURA_BLOB_DIR"));
	let bytes = std::fs::read(dir.join(format!("cumulus-aura-authorizer-{scheme}.jam")))
		.expect("built authorizer");
	let blob = jam_program_blob_common::ProgramBlob::from_bytes(&bytes).expect("JAM envelope");
	assert_eq!(blob.stack_size, 8192);
	let mut config = polkavm::Config::new();
	config.set_backend(Some(BackendKind::Interpreter));
	let engine = Engine::new(&config).expect("interpreter engine");
	let mut config = polkavm::ModuleConfig::new();
	config
		.set_page_size(4096)
		.set_aux_data_size(1 << 24)
		.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
	let program = polkavm::ProgramBlob::from_parts(blob.into()).expect("JAM program");
	let module = Module::from_blob(&engine, &config, program).expect("load guest");
	assert_eq!(module.memory_map().stack_size(), 8192);

	let (root, proof) = build_collator_proof(&[key], 0).expect("single collator");
	let config = AuthConfig {
		para_ids: vec![ParaId::new(0)],
		parachain_service: 1,
		collator_set_root: root,
		collator_set_size: 1.try_into().expect("nonzero size"),
		slot_duration: 1.try_into().expect("nonzero duration"),
	};
	let body = WorkPackageBody {
		auth_code_host: 1,
		authorizer: Authorizer {
			code_hash: sp_crypto_hashing::blake2_256(&bytes).into(),
			config: codec::Encode::encode(&config).into(),
		},
		context: RefineContext {
			anchor: Default::default(),
			anchor_slot: 0,
			state_root: Default::default(),
			beefy_root: Default::default(),
			lookup_anchor: Default::default(),
			lookup_anchor_slot: 0,
			lookup_anchor_state_root: Default::default(),
			prerequisites: Default::default(),
		},
		items: vec![WorkItem {
			service: 1,
			payload: vec![0; 36].into(),
			code_hash: Default::default(),
			refine_gas_limit: 1,
			accumulate_gas_limit: 1,
			export_count: 0,
			extrinsics: Default::default(),
			import_segments: Default::default(),
		}]
		.try_into()
		.expect("one work item fits"),
	};
	let mut token = AuthToken { signature: sign(body.signing_hash().as_bytes()), proof, key };
	assert_eq!(
		execute(&module, &body.clone().with_authorization(&token).encode()).as_deref(),
		Some(key.as_slice())
	);
	token.signature[0] ^= 1;
	assert_eq!(execute(&module, &body.with_authorization(&token).encode()), None);
}

#[test]
#[ignore = "requires built JAM blobs in AURA_BLOB_DIR"]
fn ed25519_blob_works() {
	use ed25519_dalek::{Signer, SigningKey};
	let key = SigningKey::from_bytes(&[42; 32]);
	check("ed25519", key.verifying_key().to_bytes(), |payload| key.sign(payload).to_bytes());
}

#[test]
#[ignore = "requires built JAM blobs in AURA_BLOB_DIR"]
fn sr25519_blob_works() {
	let key = schnorrkel::MiniSecretKey::from_bytes(&[42; 32])
		.expect("valid seed")
		.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
	check("sr25519", key.public.to_bytes(), |payload| {
		key.sign_simple(b"substrate", payload).to_bytes()
	});
}

use crate::parachains::penpal::api::{
	runtime_types as penpalTypes, runtime_types::staging_xcm as penpalXcm,
};
use penpalTypes::sp_weights::weight_v2::Weight;
use penpalXcm::v3::multilocation::MultiLocation;

use penpalTypes::xcm::{
	double_encoded::DoubleEncoded,
	v2::OriginKind,
	v3::{
		junctions::Junctions,
		multiasset::{AssetId::Concrete, Fungibility::Fungible, MultiAsset, MultiAssets},
		Instruction, MaybeErrorCode, WeightLimit, Xcm,
	},
	VersionedXcm,
};

pub const XCM_WEIGHT_REQUIRED: u64 = 3_000_000_000;
pub const XCM_PROOF_SIZE_REQUIRED: u64 = 100_000;
pub const BRIDGE_HUB_FEE_REQUIRED: u128 = 1000000000000;

pub fn construct_xcm_message(encoded_call: Vec<u8>) -> Box<VersionedXcm> {
	Box::new(VersionedXcm::V3(Xcm(vec![
		Instruction::UnpaidExecution {
			weight_limit: WeightLimit::Limited(Weight {
				ref_time: XCM_WEIGHT_REQUIRED,
				proof_size: XCM_PROOF_SIZE_REQUIRED,
			}),
			check_origin: None,
		},
		Instruction::Transact {
			origin_kind: OriginKind::Xcm,
			require_weight_at_most: Weight {
				ref_time: XCM_WEIGHT_REQUIRED,
				proof_size: XCM_PROOF_SIZE_REQUIRED,
			},
			call: DoubleEncoded { encoded: encoded_call },
		},
		Instruction::ExpectTransactStatus(MaybeErrorCode::Success),
	])))
}

// WithdrawAsset is not allowed in bridgehub but keep it here
pub async fn construct_xcm_message_with_fee(encoded_call: Vec<u8>) -> Box<VersionedXcm> {
	let buy_execution_fee = MultiAsset {
		id: Concrete(MultiLocation { parents: 1, interior: Junctions::Here }),
		fun: Fungible(BRIDGE_HUB_FEE_REQUIRED),
	};

	Box::new(VersionedXcm::V3(Xcm(vec![
		Instruction::WithdrawAsset(MultiAssets(vec![buy_execution_fee])),
		Instruction::BuyExecution {
			fees: MultiAsset {
				id: Concrete(MultiLocation { parents: 1, interior: Junctions::Here }),
				fun: Fungible(BRIDGE_HUB_FEE_REQUIRED),
			},
			weight_limit: WeightLimit::Unlimited,
		},
		Instruction::Transact {
			origin_kind: OriginKind::Xcm,
			require_weight_at_most: Weight {
				ref_time: XCM_WEIGHT_REQUIRED,
				proof_size: XCM_PROOF_SIZE_REQUIRED,
			},
			call: DoubleEncoded { encoded: encoded_call },
		},
	])))
}

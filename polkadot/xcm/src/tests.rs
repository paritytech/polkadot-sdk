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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

use crate::*;
use alloc::vec;

#[test]
fn encode_decode_versioned_asset_id_v3() {
	let asset_id = VersionedAssetId::V3(v3::AssetId::Abstract([1; 32]));
	let encoded = asset_id.encode();

	assert_eq!(
		encoded,
		hex_literal::hex!("03010101010101010101010101010101010101010101010101010101010101010101"),
		"encode format changed"
	);
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedAssetId::decode(&mut &encoded[..]).unwrap();
	assert_eq!(asset_id, decoded);
}

#[test]
fn encode_decode_versioned_response_v3() {
	let response = VersionedResponse::V3(v3::Response::Null);
	let encoded = response.encode();

	assert_eq!(encoded, hex_literal::hex!("0300"), "encode format changed");
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedResponse::decode(&mut &encoded[..]).unwrap();
	assert_eq!(response, decoded);
}

#[test]
fn encode_decode_versioned_response_v4() {
	let response = VersionedResponse::V4(v4::Response::Null);
	let encoded = response.encode();

	assert_eq!(encoded, hex_literal::hex!("0400"), "encode format changed");
	assert_eq!(encoded[0], 4, "bad version number");

	let decoded = VersionedResponse::decode(&mut &encoded[..]).unwrap();
	assert_eq!(response, decoded);
}

#[test]
fn encode_decode_versioned_response_v5() {
	let response = VersionedResponse::V5(v5::Response::Null);
	let encoded = response.encode();

	assert_eq!(encoded, hex_literal::hex!("0500"), "encode format changed");
	assert_eq!(encoded[0], 5, "bad version number");

	let decoded = VersionedResponse::decode(&mut &encoded[..]).unwrap();
	assert_eq!(response, decoded);
}

#[test]
fn encode_decode_versioned_location_v3() {
	let location = VersionedLocation::V3(v3::MultiLocation::new(0, v3::Junctions::Here));
	let encoded = location.encode();

	assert_eq!(encoded, hex_literal::hex!("030000"), "encode format changed");
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedLocation::decode(&mut &encoded[..]).unwrap();
	assert_eq!(location, decoded);
}

#[test]
fn encode_decode_versioned_location_v4() {
	let location = VersionedLocation::V4(v4::Location::new(0, v4::Junctions::Here));
	let encoded = location.encode();

	assert_eq!(encoded, hex_literal::hex!("040000"), "encode format changed");
	assert_eq!(encoded[0], 4, "bad version number");

	let decoded = VersionedLocation::decode(&mut &encoded[..]).unwrap();
	assert_eq!(location, decoded);
}

#[test]
fn encode_decode_versioned_location_v5() {
	let location = VersionedLocation::V5(v5::Location::new(0, v5::Junctions::Here));
	let encoded = location.encode();

	assert_eq!(encoded, hex_literal::hex!("050000"), "encode format changed");
	assert_eq!(encoded[0], 5, "bad version number");

	let decoded = VersionedLocation::decode(&mut &encoded[..]).unwrap();
	assert_eq!(location, decoded);
}

#[test]
fn encode_decode_versioned_interior_location_v3() {
	let location = VersionedInteriorLocation::V3(v3::InteriorMultiLocation::Here);
	let encoded = location.encode();

	assert_eq!(encoded, hex_literal::hex!("0300"), "encode format changed");
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedInteriorLocation::decode(&mut &encoded[..]).unwrap();
	assert_eq!(location, decoded);
}

#[test]
fn encode_decode_versioned_interior_location_v4() {
	let location = VersionedInteriorLocation::V4(v4::InteriorLocation::Here);
	let encoded = location.encode();

	assert_eq!(encoded, hex_literal::hex!("0400"), "encode format changed");
	assert_eq!(encoded[0], 4, "bad version number");

	let decoded = VersionedInteriorLocation::decode(&mut &encoded[..]).unwrap();
	assert_eq!(location, decoded);
}

#[test]
fn encode_decode_versioned_interior_location_v5() {
	let location = VersionedInteriorLocation::V5(v5::InteriorLocation::Here);
	let encoded = location.encode();

	assert_eq!(encoded, hex_literal::hex!("0500"), "encode format changed");
	assert_eq!(encoded[0], 5, "bad version number");

	let decoded = VersionedInteriorLocation::decode(&mut &encoded[..]).unwrap();
	assert_eq!(location, decoded);
}

#[test]
fn encode_decode_versioned_asset_v3() {
	let asset = VersionedAsset::V3(v3::MultiAsset::from((v3::MultiLocation::default(), 1)));
	let encoded = asset.encode();

	assert_eq!(encoded, hex_literal::hex!("030000000004"), "encode format changed");
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedAsset::decode(&mut &encoded[..]).unwrap();
	assert_eq!(asset, decoded);
}

#[test]
fn encode_decode_versioned_asset_v4() {
	let asset = VersionedAsset::V4(v4::Asset::from((v4::Location::default(), 1)));
	let encoded = asset.encode();

	assert_eq!(encoded, hex_literal::hex!("0400000004"), "encode format changed");
	assert_eq!(encoded[0], 4, "bad version number");

	let decoded = VersionedAsset::decode(&mut &encoded[..]).unwrap();
	assert_eq!(asset, decoded);
}

#[test]
fn encode_decode_versioned_asset_v5() {
	let asset = VersionedAsset::V5(v5::Asset::from((v5::Location::default(), 1)));
	let encoded = asset.encode();

	assert_eq!(encoded, hex_literal::hex!("0500000004"), "encode format changed");
	assert_eq!(encoded[0], 5, "bad version number");

	let decoded = VersionedAsset::decode(&mut &encoded[..]).unwrap();
	assert_eq!(asset, decoded);
}

#[test]
fn encode_decode_versioned_assets_v3() {
	let assets = VersionedAssets::V3(v3::MultiAssets::from(vec![
		(v3::MultiAsset::from((v3::MultiLocation::default(), 1))),
	]));
	let encoded = assets.encode();

	assert_eq!(encoded, hex_literal::hex!("03040000000004"), "encode format changed");
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedAssets::decode(&mut &encoded[..]).unwrap();
	assert_eq!(assets, decoded);
}

#[test]
fn encode_decode_versioned_assets_v4() {
	let assets = VersionedAssets::V4(v4::Assets::from(vec![
		(v4::Asset::from((v4::Location::default(), 1))),
	]));
	let encoded = assets.encode();

	assert_eq!(encoded, hex_literal::hex!("040400000004"), "encode format changed");
	assert_eq!(encoded[0], 4, "bad version number");

	let decoded = VersionedAssets::decode(&mut &encoded[..]).unwrap();
	assert_eq!(assets, decoded);
}

#[test]
fn encode_decode_versioned_assets_v5() {
	let assets = VersionedAssets::V5(v5::Assets::from(vec![
		(v5::Asset::from((v5::Location::default(), 1))),
	]));
	let encoded = assets.encode();

	assert_eq!(encoded, hex_literal::hex!("050400000004"), "encode format changed");
	assert_eq!(encoded[0], 5, "bad version number");

	let decoded = VersionedAssets::decode(&mut &encoded[..]).unwrap();
	assert_eq!(assets, decoded);
}

#[test]
fn encode_decode_versioned_xcm_v3() {
	let xcm = VersionedXcm::V3(v3::Xcm::<()>::new());
	let encoded = xcm.encode();

	assert_eq!(encoded, hex_literal::hex!("0300"), "encode format changed");
	assert_eq!(encoded[0], 3, "bad version number");

	let decoded = VersionedXcm::decode(&mut &encoded[..]).unwrap();
	assert_eq!(xcm, decoded);
}

#[test]
fn encode_decode_versioned_xcm_v4() {
	let xcm = VersionedXcm::V4(v4::Xcm::<()>::new());
	let encoded = xcm.encode();

	assert_eq!(encoded, hex_literal::hex!("0400"), "encode format changed");
	assert_eq!(encoded[0], 4, "bad version number");

	let decoded = VersionedXcm::decode(&mut &encoded[..]).unwrap();
	assert_eq!(xcm, decoded);
}

#[test]
fn encode_decode_versioned_xcm_v5() {
	let xcm = VersionedXcm::V5(v5::Xcm::<()>::new());
	let encoded = xcm.encode();

	assert_eq!(encoded, hex_literal::hex!("0500"), "encode format changed");
	assert_eq!(encoded[0], 5, "bad version number");

	let decoded = VersionedXcm::decode(&mut &encoded[..]).unwrap();
	assert_eq!(xcm, decoded);
}

// With the renaming of the crate to `staging-xcm` the naming in the metadata changed as well and
// this broke downstream users. This test ensures that the name in the metadata isn't changed.
#[test]
fn ensure_type_info_is_correct() {
	let type_info = VersionedXcm::<()>::type_info();
	assert_eq!(type_info.path.segments, vec!["xcm", "VersionedXcm"]);

	let type_info = VersionedAssetId::type_info();
	assert_eq!(type_info.path.segments, vec!["xcm", "VersionedAssetId"]);
}

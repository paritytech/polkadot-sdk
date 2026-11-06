import {beforeAll, expect, test} from "bun:test";
import {AssetState, MockNetwork, setup} from './util'

import {
	XcmV3Junction,
	XcmV3Junctions,
	XcmV3MultiassetFungibility,
	XcmV3MultiassetMultiAssetFilter,
	XcmV4AssetAssetFilter,
	XcmV3WeightLimit,
	Wnd_ahCalls,
	XcmV4Instruction,
} from "@polkadot-api/descriptors";

import {Binary, Enum} from "polkadot-api";
import {getPolkadotSigner} from "polkadot-api/signer";

import {sr25519CreateDerive} from "@polkadot-labs/hdkd";
import {
	DEV_PHRASE,
	entropyToMiniSecret,
	mnemonicToEntropy,
	ss58Address
} from "@polkadot-labs/hdkd-helpers";

import {Asset} from "./data/tokenMap";

let hdkdKeyPairAlice, hdkdKeyPairBob;
let aliceSigner, bobSigner;
let aliceAddr, bobAddr;
let westendNetwork;

let network: MockNetwork;


beforeAll(async () => {
	westendNetwork = Uint8Array.from([
		225, 67, 242, 56, 3, 172, 80, 232, 246, 248, 230, 38, 149, 209, 206, 158,
		78, 29, 104, 170, 54, 193, 205, 44, 253, 21, 52, 2, 19, 243, 66, 62,
	]);
	// Initialize HDKD key pairs and signers
	const entropy = mnemonicToEntropy(DEV_PHRASE);
	const miniSecret = entropyToMiniSecret(entropy);
	const derive = sr25519CreateDerive(miniSecret);

	hdkdKeyPairAlice = derive("//Alice");
	aliceSigner = getPolkadotSigner(hdkdKeyPairAlice.publicKey,
		"Sr25519",
		hdkdKeyPairAlice.sign,
	);
	aliceAddr = ss58Address(aliceSigner.publicKey, 42);

	hdkdKeyPairBob = derive("//Bob");
	bobSigner = getPolkadotSigner(
		hdkdKeyPairBob.publicKey,
		"Sr25519",
		hdkdKeyPairBob.sign,
	);
	bobAddr = ss58Address(bobSigner.publicKey, 42);

	console.log('setup started succesfully');
	network = await setup();
	console.log('setup finished succesfully');
});

test('test mgiration', async () => {
	console.log('before new block');
	await network.relay.context.dev.newBlock({count: 2});
	await network.assetHub.context.dev.newBlock({count: 2});
	console.log('after new block');
	await delay(600000);
});

function delay(ms: number): Promise<void> {
	return new Promise(resolve => setTimeout(resolve, ms));
}


// test("Set Asset Claimer, Trap Assets, Claim Trapped Assets", async () => {
// 	await network.assetHub.setSystemAsset([[aliceAddr, 10_000_000_000_000n]]);
// 	const [bob_balance_before] =  await network.assetHub.getSystemAsset([[bobAddr]]);
//
// 	const alice_msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum("V5", [
// 		Enum('SetHints', {
// 			hints: [
// 				Enum('AssetClaimer', {
// 					location: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
// 							network: Enum("ByGenesis", Binary.fromBytes(westendNetwork)),
// 							id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
// 						}))
// 					},
// 				})
// 			],
// 		}),
// 		Enum('WithdrawAsset', [{
// 			id: { parents: 1, interior: XcmV3Junctions.Here() },
// 			fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 		}]),
// 		Enum('ClearOrigin'),
// 	]);
// 	const alice_weight = await network.assetHub.api.apis.XcmPaymentApi.query_xcm_weight(alice_msg);
//
// 	// Transaction 1: Alice sets asset claimer to Bob and sends a trap transaction
// 	const trap_tx = network.assetHub.api.tx.PolkadotXcm.execute({
// 		message: alice_msg,
// 		max_weight: {
// 			ref_time: alice_weight.value.ref_time,
// 			proof_size: alice_weight.value.proof_size,
// 		},
// 	});
//
// 	const trap_result = await trap_tx.signAndSubmit(aliceSigner);
// 	expect(trap_result.ok).toBeTruthy();
//
// 	const bob_msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum("V4", [
// 		XcmV4Instruction.ClaimAsset({
// 			assets: [
// 				{
// 					id: { parents: 1, interior: XcmV3Junctions.Here() },
// 					fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 				},
// 			],
// 			ticket: { parents: 0, interior: XcmV3Junctions.Here() },
// 		}),
// 		XcmV4Instruction.BuyExecution({
// 			fees: {
// 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// 				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000n),
// 			},
// 			weight_limit: XcmV3WeightLimit.Unlimited(),
// 		}),
// 		XcmV4Instruction.DepositAsset({
// 			assets: XcmV3MultiassetMultiAssetFilter.Definite([
// 				{
// 					fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 					id: { parents: 1, interior: XcmV3Junctions.Here() },
// 				},
// 			]),
// 			beneficiary: {
// 				parents: 0,
// 				interior: XcmV3Junctions.X1(
// 					XcmV3Junction.AccountId32({
// 						network: undefined,
// 						id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
// 					})
// 				),
// 			},
// 		}),
// 	]);
// 	const bob_weight = await network.assetHub.api.apis.XcmPaymentApi.query_xcm_weight(bob_msg);
//
// 	// Transaction 2: Bob claims trapped assets.
// 	const bob_claim_tx = network.assetHub.api.tx.PolkadotXcm.execute({
// 		message: bob_msg,
// 		max_weight: {
// 			ref_time: bob_weight.value.ref_time,
// 			proof_size: bob_weight.value.proof_size,
// 		},
// 	});
//
// 	const claim_result = await bob_claim_tx.signAndSubmit(bobSigner);
// 	expect(claim_result.ok).toBeTruthy();
//
// 	const [bob_balance_after] = await network.assetHub.getSystemAsset([[bobAddr]]);
// 	expect(bob_balance_after > bob_balance_before).toBeTruthy();
// });
//
// test("Initiate Teleport XCM v4 (AH -> RC)", async () => {
// 	await network.assetHub.setSystemAsset([[aliceAddr, 10_000_000_000_000n]]);
//
// 	const [bob_balance_before] =  await network.relay.getSystemAsset([[bobAddr]]);
// 	const deposit_amount = 5_000_000_000_000n;
//
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum("V4", [
// 		XcmV4Instruction.WithdrawAsset([{
// 			id: { parents: 1, interior: XcmV3Junctions.Here() },
// 			fun: XcmV3MultiassetFungibility.Fungible(7_000_000_000_000n),
// 		}]),
// 		XcmV4Instruction.SetFeesMode({
// 			jit_withdraw: true,
// 		}),
// 		XcmV4Instruction.InitiateTeleport({
// 			assets: XcmV4AssetAssetFilter.Wild({ type: "All", value: undefined }),
// 			dest: {
// 				parents: 1,
// 				interior: XcmV3Junctions.Here(),
// 			},
// 			xcm: [
// 				XcmV4Instruction.BuyExecution({
// 					fees: {
// 						id: { parents: 0, interior: XcmV3Junctions.Here() },
// 						fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
// 					},
// 					weight_limit: XcmV3WeightLimit.Unlimited(),
// 				}),
// 				XcmV4Instruction.DepositAsset({
// 					assets: XcmV3MultiassetMultiAssetFilter.Definite([{
// 						fun: XcmV3MultiassetFungibility.Fungible(deposit_amount),
// 						id: { parents: 0, interior: XcmV3Junctions.Here() },
// 					}]),
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(
// 							XcmV3Junction.AccountId32({
// 								network: undefined,
// 								id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
// 							})
// 						),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
// 	const weight = await network.assetHub.api.apis.XcmPaymentApi.query_xcm_weight(msg);
//
// 	const ahToWnd = await network.assetHub.api.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await network.relay.context.ws.send('dev_newBlock', [{ count: 1 }])
// 	const [bob_balance_after] = await network.relay.getSystemAsset([[bobAddr]]);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });
//
// test("Initiate Teleport XCM v5 (AH -> RC)", async () => {
// 	await network.assetHub.setSystemAsset([[aliceAddr, 10_000_000_000_000n]]);
//
// 	const [bob_balance_before] =  await network.relay.getSystemAsset([[bobAddr]]);
// 	const deposit_amount = 1_000_000_000_000n;
//
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// 		Enum('WithdrawAsset', [
// 			{
// 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// 				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
// 			},
// 		]),
// 		Enum('PayFees', {
// 			asset: {
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 			},
// 		}),
// 		Enum('InitiateTransfer', {
// 			destination: {
// 				parents: 1,
// 				interior: XcmV3Junctions.Here(),
// 			},
// 			// optional field. an example of usage:
// 			// remote_fees: Enum('Teleport', {
// 			// 	type: 'Wild',
// 			// 	value: {
// 			// 		type: 'All',
// 			// 		value: undefined,
// 			// 	},
// 			// }),
// 			preserve_origin: false,
// 			assets: [
// 				Enum('Teleport', {
// 					type: 'Wild',
// 					value: {
// 						type: 'All',
// 						value: undefined,
// 					},
// 				}),
// 			],
// 			remote_xcm: [
// 				Enum('PayFees', {
// 					asset: {
// 						id: {
// 							parents: 0,
// 							interior: XcmV3Junctions.Here(),
// 						},
// 						fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 					},
// 				}),
// 				Enum('DepositAsset', {
// 					assets: XcmV3MultiassetMultiAssetFilter.Definite([{
// 						fun: XcmV3MultiassetFungibility.Fungible(deposit_amount),
// 						id: { parents: 0, interior: XcmV3Junctions.Here() },
// 					}]),
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(
// 							XcmV3Junction.AccountId32({
// 								network: undefined,
// 								id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
// 							})
// 						),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
// 	const weight = await network.assetHub.api.apis.XcmPaymentApi.query_xcm_weight(msg);
//
// 	const ah_to_wnd = network.assetHub.api.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
//
// 	const r = await ah_to_wnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await network.relay.context.ws.send('dev_newBlock', [{ count: 1 }])
// 	const [bob_balance_after] = await network.relay.getSystemAsset([[bobAddr]]);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });
//
// test("Initiate Teleport (AH -> RC) with remote fees", async () => {
// 	await network.assetHub.setSystemAsset([[aliceAddr, 10_000_000_000_000n]]);
//
// 	const [bob_balance_before] =  await network.relay.getSystemAsset([[bobAddr, 'Relay']]);
// 	const deposit_amount = 2_000_000_000_000n;
//
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// 		Enum('WithdrawAsset', [
// 			{
// 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// 				fun: XcmV3MultiassetFungibility.Fungible(6_000_000_000_000n),
// 			},
// 		]),
// 		Enum('PayFees', {
// 			asset: {
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(3_000_000_000_000n),
// 			}
// 		}),
// 		Enum('InitiateTransfer', {
// 			destination: {
// 				parents: 1,
// 				interior: XcmV3Junctions.Here(),
// 			},
// 			// optional field. an example of usage:
// 			remote_fees: {
// 				type: 'Teleport',
// 				value: {
// 					type: 'Definite',
// 					value: [
// 						{
// 							id: {
// 								parents: 1,
// 								interior: XcmV3Junctions.Here(),
// 							},
// 							fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 						},
// 					],
// 				},
// 			},
// 			preserve_origin: false,
// 			assets: [Enum('Teleport', {
// 				type: 'Wild',
// 				value: {
// 					type: 'All',
// 					value: undefined,
// 				},
// 			})],
// 			remote_xcm: [
// 				Enum('DepositAsset', {
// 					assets: XcmV3MultiassetMultiAssetFilter.Wild({
// 						type: 'All',
// 						value: undefined,
// 					}),
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
// 							network: undefined,
// 							id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
// 						})),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
// 	const weight = await network.assetHub.api.apis.XcmPaymentApi.query_xcm_weight(msg);
//
// 	const ah_to_wnd = network.assetHub.api.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 		},
// 	);
// 	const r = await ah_to_wnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await network.relay.context.ws.send('dev_newBlock', [{ count: 1 }])
// 	const [bob_balance_after] = await network.relay.getSystemAsset([[bobAddr]]);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });
//
// test("Reserve Asset Transfer (local) of USDT from Asset Hub `Alice` to Penpal `Alice`", async () => {
// 	const [alice_balance_before] = await network.parachain.getAssets([[aliceAddr, Asset.USDT, AssetState.Foreign]]);
//
// 	await network.assetHub.setSystemAsset([[aliceAddr, 10_000_000_000_000n]]);
// 	await network.assetHub.setAssets([
// 		[aliceAddr, Asset.USDT, AssetState.Local, 10_000_000_000_000n],
// 		[network.paraSovAccOnAssetHub, Asset.USDT, AssetState.Local, 10_000_000_000_000n],
// 	]);
//
// 	await network.assetHub.setXcmVersion(5);
// 	await network.parachain.setXcmVersion(5);
//
// 	const withdraw_usdt = 100_000_000n;
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// 		Enum('WithdrawAsset', [
// 			{
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
// 			},
// 		]),
// 		Enum('PayFees', {
// 			asset: {
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 			},
// 		}),
// 		Enum('TransferReserveAsset', {
// 			assets: [
// 				{
// 					id: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X2([
// 							XcmV3Junction.PalletInstance(50),
// 							XcmV3Junction.GeneralIndex(1984n)]),
// 					},
// 					fun: XcmV3MultiassetFungibility.Fungible(withdraw_usdt),
// 				},
// 				{
// 					id: {
// 						parents: 1,
// 						interior: XcmV3Junctions.Here(),
// 					},
// 					fun: XcmV3MultiassetFungibility.Fungible(4_000_000_000_000n),
// 				},
// 			],
// 			dest: {
// 				parents: 1,
// 				interior: XcmV3Junctions.X1(
// 					XcmV3Junction.Parachain(2042),
// 				),
// 			},
// 			xcm: [
// 				Enum('PayFees', {
// 					asset: {
// 						id: {
// 							parents: 1,
// 							interior: XcmV3Junctions.Here(),
// 						},
// 						fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000n),
// 					},
// 				}),
// 				Enum('DepositAsset', {
// 					// some WND might get trapped bc of extra fungibles in PayFees above ^
// 					assets: XcmV3MultiassetMultiAssetFilter.Wild({
// 						type: 'All',
// 						value: undefined,
// 					}),
//
// 					// ===================== GRANULAR VERSIONS =====================
// 					// assets: XcmV4AssetAssetFilter.Definite([{
// 					// 	id: {
// 					// 		parents: 1,
// 					// 		interior: XcmV3Junctions.Here(),
// 					// 	},
// 					// 	fun: XcmV3MultiassetFungibility.Fungible(3_995_000_000_000n),
// 					// }]),
// 					//
// 					// assets: XcmV4AssetAssetFilter.Definite([{
// 					// 	id: {
// 					// 		parents: 1,
// 					// 		interior: XcmV3Junctions.X3([
// 					// 			XcmV3Junction.Parachain(1000),
// 					// 			XcmV3Junction.PalletInstance(50),
// 					// 			XcmV3Junction.GeneralIndex(1984n)]),
// 					// 	},
// 					// 	fun: XcmV3MultiassetFungibility.Fungible(100_000_000n),
// 					// }]),
// 					// ===================== END =====================
//
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
// 							network: undefined,
// 							id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
// 						})),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
//
// 	const ahToWnd = network.assetHub.api.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: 100_000_000_000n, proof_size: 1_000_000n },
// 		},
// 	);
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await network.parachain.context.ws.send('dev_newBlock', [{ count: 1 }])
// 	const [alice_balance_after] = await network.parachain.getAssets([[aliceAddr, Asset.USDT, AssetState.Foreign]]);
// 	expect(alice_balance_after - alice_balance_before).toBe(withdraw_usdt);
// });
//
// test("InitiateReserveWithdraw USDT from Penpal `Alice` to Asset Hub `Bob`", async () => {
// 	const withdraw_usdt = 177_777_000n;
// 	const system_asset_amount = 25_000_000_000_000n;
//
// 	await network.assetHub.setXcmVersion(5);
// 	await network.parachain.setXcmVersion(5);
//
// 	await network.parachain.setSystemAsset([[aliceAddr, system_asset_amount]]);
// 	await network.parachain.setAssets([
// 		[aliceAddr, Asset.USDT, AssetState.Foreign, withdraw_usdt],
// 		[aliceAddr, Asset.WND, AssetState.Foreign, system_asset_amount]
// 	]);
//
// 	await network.assetHub.setAssets([
// 		[network.paraSovAccOnAssetHub, Asset.USDT, AssetState.Local, withdraw_usdt],
// 	]);
// 	await network.assetHub.setSystemAsset([[network.paraSovAccOnAssetHub, system_asset_amount]]);
//
// 	const [alice_balance_before] = await network.assetHub.getAssets([[aliceAddr, Asset.USDT, AssetState.Local]]);
// 	const msg /*: Wnd_ahCalls['PolkadotXcm']['execute']['message']*/ = Enum('V5', [
// 		Enum('WithdrawAsset', [
// 			{
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(3_995_000_000_000n),
// 			},
// 			{
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.X3([
// 						XcmV3Junction.Parachain(1000),
// 						XcmV3Junction.PalletInstance(50),
// 						XcmV3Junction.GeneralIndex(1984n),
// 					]),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(withdraw_usdt),
// 			},
// 		]),
// 		Enum('PayFees', {
// 			asset: {
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 			},
// 		}),
// 		Enum('InitiateReserveWithdraw' , {
// 			assets: XcmV4AssetAssetFilter.Wild({
// 				type: 'All',
// 				value: undefined,
// 			}),
// 			reserve: {
// 				parents: 1,
// 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1000)),
// 			},
// 			xcm: [
// 				Enum('PayFees', {
// 					asset: {
// 						id: {
// 							parents: 1,
// 							interior: XcmV3Junctions.Here(),
// 						},
// 						fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
// 					},
// 				}),
// 				Enum('DepositAsset', {
// 					assets: XcmV3MultiassetMultiAssetFilter.Wild({
// 						type: 'All',
// 						value: undefined,
// 					}),
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
// 							network: undefined,
// 							id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
// 						})),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
//
//
// 	const penpal_to_ah = network.parachain.api.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: 100_000_000_000n, proof_size: 1_000_000n },
// 		},
// 	);
// 	const r = await penpal_to_ah.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await network.assetHub.context.ws.send('dev_newBlock', [{ count: 1 }])
//
// 	const [alice_balance_after] = await network.assetHub.getAssets([[aliceAddr, Asset.USDT, AssetState.Local]]);
// 	expect(alice_balance_after - alice_balance_before).toBe(withdraw_usdt);
// });
//
// // test("Teleport and Transact from Westend's Asset Hub to Penpal", async () => {
// // 	const remarkWithEventCalldata = await PenpalApi.tx.System.remark_with_event({
// // 		remark: Binary.fromText("Hello, World!"),
// // 	}).getEncodedData();
// //
// // 	PenpalApi.event.System.Remarked.watch().forEach((e) =>
// // 		console.log(
// // 			`\nBlock: ${e.meta.block.number}\nSender: ${
// // 				e.payload.sender
// // 			}\nHash: ${e.payload.hash.asHex()}\n`
// // 		)
// // 	);
// //
// // 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// // 		XcmV4Instruction.WithdrawAsset([
// // 			{
// // 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// // 				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
// // 			},
// // 		]),
// // 		Enum('PayFees', {
// // 			asset: {
// // 				id: {
// // 					parents: 1,
// // 					interior: XcmV3Junctions.Here(),
// // 				},
// // 				fun: XcmV3MultiassetFungibility.Fungible(4_000_000_000_000n),
// // 			},
// // 		}),
// // 		Enum('InitiateTransfer', {
// // 			destination: {
// // 				parents: 1,
// // 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2042)),
// // 			},
// // 			remote_fees: {
// // 				type: 'Teleport',
// // 				value: {
// // 					type: 'Wild',
// // 					value: {
// // 						type: 'All',
// // 						value: undefined,
// // 					},
// // 				},
// // 			},
// // 			preserve_origin: true,
// // 			assets: [],
// // 			remote_xcm: [
// // 				{
// // 					type: 'Transact',
// // 					value: {
// // 						origin_kind: XcmV2OriginKind.SovereignAccount(),
// // 						call: remarkWithEventCalldata,
// // 					},
// // 				},
// // 			],
// // 		}),
// // 	]);
// //
// // 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
// // 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// // 		msg,
// // 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// // 	});
// // 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// // 	expect(r.ok).toBeTruthy();
// // });
// //
// // test("Initiate Teleport XCM v5 from Westend's Asset Hub to Westend People w/ InitiateTeleport", async () => {
// // 	const message: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// // 		XcmV4Instruction.WithdrawAsset([
// // 			{
// // 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// // 				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
// // 			},
// // 		]),
// // 		Enum('PayFees', {
// // 			asset: {
// // 				id: {
// // 					parents: 1,
// // 					interior: XcmV3Junctions.Here(),
// // 				},
// // 				fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
// // 			},
// // 		}),
// // 		XcmV4Instruction.InitiateTeleport({
// // 			assets: XcmV4AssetAssetFilter.Wild({ type: 'All', value: undefined }),
// // 			dest: {
// // 				parents: 1,
// // 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1004)),
// // 			},
// // 			xcm: [
// // 				XcmV4Instruction.BuyExecution({
// // 					fees: {
// // 						id: { parents: 1, interior: XcmV3Junctions.Here() },
// // 						fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
// // 					},
// // 					weight_limit: XcmV3WeightLimit.Unlimited(),
// // 				}),
// // 				XcmV4Instruction.DepositAsset({
// // 					assets: XcmV4AssetAssetFilter.Wild({ type: 'All', value: undefined }),
// // 					beneficiary: {
// // 						parents: 0,
// // 						interior: XcmV3Junctions.X1(
// // 							XcmV3Junction.AccountId32({
// // 								network: undefined,
// // 								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
// // 							})
// // 						),
// // 					},
// // 				}),
// // 			],
// // 		}),
// // 	]);
// // 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(message);
// //
// // 	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
// // 		message,
// // 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// // 	});
// // 	const r = await ahToPpl.signAndSubmit(aliceSigner);
// // 	expect(r).toBeTruthy();
// // });
// //
// // test("Initiate Teleport XCM v5 from Westend's Asset Hub to Westend People w/ InitiateTransfer", async () => {
// // 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// // 		XcmV4Instruction.WithdrawAsset([
// // 			{
// // 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// // 				fun: XcmV3MultiassetFungibility.Fungible(10_000_000_000_000n),
// // 			},
// // 		]),
// // 		Enum('PayFees', {
// // 			asset: {
// // 				id: {
// // 					parents: 1,
// // 					interior: XcmV3Junctions.Here(),
// // 				},
// // 				fun: XcmV3MultiassetFungibility.Fungible(665_000_000_000n),
// // 			},
// // 		}),
// // 		Enum('InitiateTransfer', {
// // 			destination: {
// // 				parents: 1,
// // 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1004)),
// // 			},
// // 			remote_fees: {
// // 				type: 'Teleport',
// // 				value: {
// // 					type: 'Definite',
// // 					value: [
// // 						{
// // 							id: {
// // 								parents: 1,
// // 								interior: XcmV3Junctions.Here(),
// // 							},
// // 							fun: XcmV3MultiassetFungibility.Fungible(365_000_000_000n),
// // 						},
// // 					],
// // 				},
// // 			},
// // 			preserve_origin: true,
// // 			assets: [
// // 				Enum('Teleport', {
// // 					type: 'Wild',
// // 					value: {
// // 						type: 'All',
// // 						value: undefined,
// // 					},
// // 				}),
// // 			],
// // 			remote_xcm: [
// // 				XcmV4Instruction.DepositAsset({
// // 					assets: XcmV4AssetAssetFilter.Wild({
// // 						type: 'All',
// // 						value: undefined,
// // 					}),
// // 					beneficiary: {
// // 						parents: 0,
// // 						interior: XcmV3Junctions.X1(
// // 							XcmV3Junction.AccountId32({
// // 								network: undefined,
// // 								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
// // 							})
// // 						),
// // 					},
// // 				}),
// // 			],
// // 		}),
// // 	]);
// //
// // 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
// // 	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
// // 		message: msg,
// // 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// // 	});
// //
// // 	const r = await ahToPpl.signAndSubmit(aliceSigner);
// // 	expect(r).toBeTruthy();
// //
// // 	expect(await getFreeBalance(AHApi, CONFIG.KEYS.ALICE)).toBeLessThan(
// // 		1_000_000_000_000_000n - 10_000_000_000_000n
// // 	);
// // });

import { test, beforeAll, expect } from "bun:test";
import { CONFIG } from "./config";
import { createPolkadotClient, getFreeBalance } from './util'

import {
	wnd_ah,
	Wnd_ahCalls,
	wnd_penpal,
	wnd_rc,
	XcmV3Junctions,
	XcmV3Junction,
	XcmV3MultiassetFungibility,
	XcmV4Instruction,
	XcmV3WeightLimit,
	XcmV3MultiassetMultiAssetFilter,
	XcmV4AssetAssetFilter,
	XcmV2OriginKind,
} from "@polkadot-api/descriptors";

import { Binary, Enum } from "polkadot-api";
import { getPolkadotSigner } from "polkadot-api/signer";

import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";

// import { WsProvider } from '@polkadot/rpc-provider';
import { setupNetworks, type NetworkContext } from "@acala-network/chopsticks-testing"
import { createClient } from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";

let parachain: NetworkContext, polkadot: NetworkContext, assetHub: NetworkContext;
let ah_provider, rc_provider;
// todo unify, small case start
let AHApi, PenpalApi, rcApi;

let hdkdKeyPairAlice, hdkdKeyPairBob;
let aliceSigner, bobSigner;


beforeAll(async () => {
	// Initialize HDKD key pairs and signers
	const entropy = mnemonicToEntropy(DEV_PHRASE);
	const miniSecret = entropyToMiniSecret(entropy);
	const derive = sr25519CreateDerive(miniSecret);

	({ parachain, polkadot, assetHub }  = await setupNetworks({
		// todo how to specify penpal explicitly, not a general parachain?
		parachain: {
			endpoint: CONFIG.WS_ADDRESSES.PENPAL,
			// move hardcoded values to .env
			'wasm-override': '../../wasms/penpal_runtime.compact.compressed.wasm',
			'runtime-log-level': 5,
			db: './db.sqlite',
			port: 8006,
		},
		// todo is there a westend option? I tried setting it up and got an error.
		// todo how  and where to find it?
		polkadot: {
			endpoint: CONFIG.WS_ADDRESSES.RC,
			'wasm-override': '../../wasms/westend_runtime.compact.compressed.wasm',
			'runtime-log-level': 5,
			db: './db.sqlite',
			port: 8007,
		},
		assetHub: {
			endpoint: CONFIG.WS_ADDRESSES.AH,
			// move hardcoded values to .env
			'wasm-override': '../../wasms/asset_hub_westend_runtime.compact.compressed.wasm',
			'runtime-log-level': 5,
			db: './db.sqlite',
			port: 8008,
		},
	}));

	// init clients
	AHApi = createPolkadotClient(assetHub.ws.endpoint, wnd_ah);
	PenpalApi = createPolkadotClient(parachain.ws.endpoint, wnd_penpal);
	rcApi = createPolkadotClient(polkadot.ws.endpoint, wnd_rc);


	// const penpalClient = createClient(getWsProvider(parachain.ws.endpoint));
	// PenpalApi = penpalClient.getTypedApi(wnd_penpal);
	// const rcClient = createClient(getWsProvider(polkadot.ws.endpoint));
	// rcApi = rcClient.getTypedApi(wnd_rc);
	// const assetHubClient = createClient(getWsProvider(assetHub.ws.endpoint));
	// AHApi = assetHubClient.getTypedApi(wnd_ah);


	hdkdKeyPairAlice = derive("//Alice");
	aliceSigner = getPolkadotSigner(hdkdKeyPairAlice.publicKey,
		"Sr25519",
		hdkdKeyPairAlice.sign,
	);

	hdkdKeyPairBob = derive("//Bob");
	bobSigner = getPolkadotSigner(
		hdkdKeyPairBob.publicKey,
		"Sr25519",
		hdkdKeyPairBob.sign,
	);


	// init providers
	// ah_provider = new WsProvider(CONFIG.WS_ADDRESSES.AH);
	// rc_provider = new WsProvider(CONFIG.WS_ADDRESSES.RC);
	//
	// await Promise.all([
	// 	ah_provider.isReady,
	// 	rc_provider.isReady,
	// ]);
});


// test("Set Asset Claimer, Trap Assets, Claim Trapped Assets", async () => {
// 	const bobBalanceBefore = await getFreeBalance(AHApi, CONFIG.KEYS.BOB);
//
// 	console.log("test start")
//
// 	const alice_msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum("V5", [
// 		Enum('SetHints', {
// 			hints: [
// 				Enum('AssetClaimer', {
// 					location: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
// 							network: Enum("ByGenesis", Binary.fromBytes(CONFIG.WESTEND_NETWORK)),
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
// 	const alice_weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(alice_msg);
//
// 	// Transaction 1: Alice sets asset claimer to Bob and sends a trap transaction
// 	const trapTx = AHApi.tx.PolkadotXcm.execute({
// 		message: alice_msg,
// 		max_weight: {
// 			ref_time: alice_weight.value.ref_time,
// 			proof_size: alice_weight.value.proof_size,
// 		},
// 	});
//
// 	const trapResult = await trapTx.signAndSubmit(aliceSigner);
// 	expect(trapResult.ok).toBeTruthy();
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
// 	const bob_weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(bob_msg);
//
// 	// Transaction 2: Bob claims trapped assets.
// 	const bobClaimTx = AHApi.tx.PolkadotXcm.execute({
// 		message: bob_msg,
// 		max_weight: {
// 			ref_time: bob_weight.value.ref_time,
// 			proof_size: bob_weight.value.proof_size,
// 		},
// 	});
//
// 	const claimResult = await bobClaimTx.signAndSubmit(bobSigner);
// 	expect(claimResult.ok).toBeTruthy();
//
// 	const bobBalanceAfter = await getFreeBalance(AHApi, CONFIG.KEYS.BOB);
// 	expect(bobBalanceAfter > bobBalanceBefore).toBeTruthy();
// });

test("Initiate Teleport XCM v4 (AH -> RC)", async () => {
	// set storage for Alice
	const bob_balance_before =  await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
	const deposit_amount = 5_000_000_000_000n;

	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum("V4", [
		XcmV4Instruction.WithdrawAsset([{
			id: { parents: 1, interior: XcmV3Junctions.Here() },
			fun: XcmV3MultiassetFungibility.Fungible(7_000_000_000_000n),
		}]),
		XcmV4Instruction.SetFeesMode({
			jit_withdraw: true,
		}),
		XcmV4Instruction.InitiateTeleport({
			assets: XcmV4AssetAssetFilter.Wild({ type: "All", value: undefined }),
			dest: {
				parents: 1,
				interior: XcmV3Junctions.Here(),
			},
			xcm: [
				XcmV4Instruction.BuyExecution({
					fees: {
						id: { parents: 0, interior: XcmV3Junctions.Here() },
						fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
					},
					weight_limit: XcmV3WeightLimit.Unlimited(),
				}),
				XcmV4Instruction.DepositAsset({
					assets: XcmV3MultiassetMultiAssetFilter.Definite([{
						fun: XcmV3MultiassetFungibility.Fungible(deposit_amount),
						id: { parents: 0, interior: XcmV3Junctions.Here() },
					}]),
					beneficiary: {
						parents: 0,
						interior: XcmV3Junctions.X1(
							XcmV3Junction.AccountId32({
								network: undefined,
								id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
							})
						),
					},
				}),
			],
		}),
	]);
	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);

	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
		message: msg,
		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
	});

	const r = await ahToWnd.signAndSubmit(aliceSigner);
	expect(r).toBeTruthy();

	await polkadot.ws.send('dev_newBlock', [{ count: 1 }])
	const bob_balance_after = await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
});

// test("Initiate Teleport XCM v5 (AH -> RC)", async () => {
// 	const bob_balance_before =  await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
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
// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
//
// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
//
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await rc_provider.send('dev_newBlock', [{ count: 1 }])
// 	const bob_balance_after = await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });

// test("Initiate Teleport (AH -> RC) with remote fees", async () => {
// 	const bob_balance_before =  await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
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
// 				fun: XcmV3MultiassetFungibility.Fungible(2_000_000_000_000n),
// 			}
// 		}),
// 		Enum('InitiateTransfer', {
// 			destination: {
// 				parents: 1,
// 				interior: XcmV3Junctions.Here(),
// 			},
// 			// optional field. an example of usage:
// 			remote_fees: Enum('Teleport', {
// 				type: 'Wild',
// 				value: {
// 					type: 'All',
// 					value: undefined,
// 				},
// 			}),
// 			preserve_origin: false,
// 			assets: [Enum('Teleport', {
// 				type: 'Wild',
// 				value: {
// 					type: 'All',
// 					value: undefined,
// 				},
// 			})],
// 			remote_xcm: [
// 				// something is wrong here!
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
// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
//
// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 		},
// 	);
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	await rc_provider.send('dev_newBlock', [{ count: 1 }])
// 	const bob_balance_after = await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });

// test("Reserve Asset Transfer (local) of USDT from Asset Hub `Alice` to Penpal `Alice`", async () => {
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
// 					fun: XcmV3MultiassetFungibility.Fungible(100_000_000n),
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
// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: 100_000_000_000n, proof_size: 1_000_000n },
// 		},
// 	);
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
// });
//
// // this test scenario works together with the previous one.
// // previous test serves as a set-up for this one.
// test("InitiateReserveWithdraw USDT from Penpal `Alice` to Asset Hub `Bob`", async () => {
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
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
// 				fun: XcmV3MultiassetFungibility.Fungible(70_000_000n),
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
// 							id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
// 						})),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
//
//
// 	const penpalToAH = PenpalApi.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: 100_000_000_000n, proof_size: 1_000_000n },
// 		},
// 	);
// 	const r = await penpalToAH.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
// });
//
// test("Teleport and Transact from Westend's Asset Hub to Penpal", async () => {
// 	const remarkWithEventCalldata = await PenpalApi.tx.System.remark_with_event({
// 		remark: Binary.fromText("Hello, World!"),
// 	}).getEncodedData();
//
// 	PenpalApi.event.System.Remarked.watch().forEach((e) =>
// 		console.log(
// 			`\nBlock: ${e.meta.block.number}\nSender: ${
// 				e.payload.sender
// 			}\nHash: ${e.payload.hash.asHex()}\n`
// 		)
// 	);
//
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// 		XcmV4Instruction.WithdrawAsset([
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
// 				fun: XcmV3MultiassetFungibility.Fungible(4_000_000_000_000n),
// 			},
// 		}),
// 		Enum('InitiateTransfer', {
// 			destination: {
// 				parents: 1,
// 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2042)),
// 			},
// 			remote_fees: {
// 				type: 'Teleport',
// 				value: {
// 					type: 'Wild',
// 					value: {
// 						type: 'All',
// 						value: undefined,
// 					},
// 				},
// 			},
// 			preserve_origin: true,
// 			assets: [],
// 			remote_xcm: [
// 				{
// 					type: 'Transact',
// 					value: {
// 						origin_kind: XcmV2OriginKind.SovereignAccount(),
// 						call: remarkWithEventCalldata,
// 					},
// 				},
// 			],
// 		}),
// 	]);
//
// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 		msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r.ok).toBeTruthy();
// });
//
// test("Initiate Teleport XCM v5 from Westend's Asset Hub to Westend People w/ InitiateTeleport", async () => {
// 	const message: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// 		XcmV4Instruction.WithdrawAsset([
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
// 				fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
// 			},
// 		}),
// 		XcmV4Instruction.InitiateTeleport({
// 			assets: XcmV4AssetAssetFilter.Wild({ type: 'All', value: undefined }),
// 			dest: {
// 				parents: 1,
// 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1004)),
// 			},
// 			xcm: [
// 				XcmV4Instruction.BuyExecution({
// 					fees: {
// 						id: { parents: 1, interior: XcmV3Junctions.Here() },
// 						fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
// 					},
// 					weight_limit: XcmV3WeightLimit.Unlimited(),
// 				}),
// 				XcmV4Instruction.DepositAsset({
// 					assets: XcmV4AssetAssetFilter.Wild({ type: 'All', value: undefined }),
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(
// 							XcmV3Junction.AccountId32({
// 								network: undefined,
// 								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
// 							})
// 						),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(message);
//
// 	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
// 		message,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
// 	const r = await ahToPpl.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
// });
//
// test("Initiate Teleport XCM v5 from Westend's Asset Hub to Westend People w/ InitiateTransfer", async () => {
// 	const msg: Wnd_ahCalls['PolkadotXcm']['execute']['message'] = Enum('V5', [
// 		XcmV4Instruction.WithdrawAsset([
// 			{
// 				id: { parents: 1, interior: XcmV3Junctions.Here() },
// 				fun: XcmV3MultiassetFungibility.Fungible(10_000_000_000_000n),
// 			},
// 		]),
// 		Enum('PayFees', {
// 			asset: {
// 				id: {
// 					parents: 1,
// 					interior: XcmV3Junctions.Here(),
// 				},
// 				fun: XcmV3MultiassetFungibility.Fungible(665_000_000_000n),
// 			},
// 		}),
// 		Enum('InitiateTransfer', {
// 			destination: {
// 				parents: 1,
// 				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1004)),
// 			},
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
// 							fun: XcmV3MultiassetFungibility.Fungible(365_000_000_000n),
// 						},
// 					],
// 				},
// 			},
// 			preserve_origin: true,
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
// 				XcmV4Instruction.DepositAsset({
// 					assets: XcmV4AssetAssetFilter.Wild({
// 						type: 'All',
// 						value: undefined,
// 					}),
// 					beneficiary: {
// 						parents: 0,
// 						interior: XcmV3Junctions.X1(
// 							XcmV3Junction.AccountId32({
// 								network: undefined,
// 								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
// 							})
// 						),
// 					},
// 				}),
// 			],
// 		}),
// 	]);
//
// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
// 	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
//
// 	const r = await ahToPpl.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
//
// 	expect(await getFreeBalance(AHApi, CONFIG.KEYS.ALICE)).toBeLessThan(
// 		1_000_000_000_000_000n - 10_000_000_000_000n
// 	);
// });

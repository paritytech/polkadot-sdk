import { beforeAll, expect, test } from "bun:test";
import { CONFIG } from "./config";
import {
	assert,
	type ClientContext,
	createPolkadotClient,
	getFreeBalance,
	waitForFinalizedBlocks,
} from "./util";

import {
	MultiAddress,
	type Wnd_ahCalls,
	XcmV2OriginKind,
	XcmV3Junction,
	XcmV3Junctions,
	XcmV3MaybeErrorCode,
	XcmV3MultiassetFungibility,
	XcmV3MultiassetMultiAssetFilter,
	XcmV3WeightLimit,
	XcmV4AssetAssetFilter,
	XcmV4AssetWildAsset,
	XcmV4Instruction,
	XcmV5AssetFilter,
	XcmV5Instruction,
	XcmV5Junction,
	XcmV5Junctions,
	XcmV5WildAsset,
	XcmVersionedLocation,
	XcmVersionedXcm,
	wnd_ah,
	wnd_penpal,
	wnd_people,
	wnd_rc,
} from "@polkadot-api/descriptors";

import {
	Binary,
	type ChainDefinition,
	Enum,
	type PolkadotClient,
	type PolkadotSigner,
	type TxFinalizedPayload,
	type TypedApi,
} from "polkadot-api";
import { getPolkadotSigner } from "polkadot-api/signer";

import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
	DEV_PHRASE,
	type KeyPair,
	entropyToMiniSecret,
	mnemonicToEntropy,
} from "@polkadot-labs/hdkd-helpers";

import { WsProvider } from "@polkadot/rpc-provider";

let ahProvider: WsProvider;
let rcProvider: WsProvider;
let assetHubChainCtx: ClientContext<typeof wnd_ah>;
let penpalChainCtx: ClientContext<typeof wnd_penpal>;
let relayChainCtx: ClientContext<typeof wnd_rc>;
let peopleChainCtx: ClientContext<typeof wnd_people>;

let hdkdKeyPairAlice: KeyPair;
let hdkdKeyPairBob: KeyPair;
let aliceSigner: PolkadotSigner;
let bobSigner: PolkadotSigner;

beforeAll(async () => {
	// Initialize HDKD key pairs and signers
	const entropy = mnemonicToEntropy(DEV_PHRASE);
	const miniSecret = entropyToMiniSecret(entropy);
	const derive = sr25519CreateDerive(miniSecret);

	hdkdKeyPairAlice = derive("//Alice");
	aliceSigner = getPolkadotSigner(
		hdkdKeyPairAlice.publicKey,
		"Sr25519",
		hdkdKeyPairAlice.sign,
	);

	hdkdKeyPairBob = derive("//Bob");

	bobSigner = getPolkadotSigner(
		hdkdKeyPairBob.publicKey,
		"Sr25519",
		hdkdKeyPairBob.sign,
	);

	// init clients
	assetHubChainCtx = createPolkadotClient(CONFIG.WS_ADDRESSES.AH, wnd_ah);
	penpalChainCtx = createPolkadotClient(CONFIG.WS_ADDRESSES.PENPAL, wnd_penpal);
	relayChainCtx = createPolkadotClient(CONFIG.WS_ADDRESSES.RC, wnd_rc);
	peopleChainCtx = createPolkadotClient(CONFIG.WS_ADDRESSES.PEOPLE, wnd_people);

	// init providers
	ahProvider = new WsProvider(CONFIG.WS_ADDRESSES.AH);
	rcProvider = new WsProvider(CONFIG.WS_ADDRESSES.RC);

	await Promise.all([ahProvider.isReady, rcProvider.isReady]);
});

// test("Set Asset Claimer, Trap Assets, Claim Trapped Assets", async () => {
// 	const bobBalanceBefore = await getFreeBalance(AHApi, CONFIG.KEYS.BOB);

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

// 	// Transaction 1: Alice sets asset claimer to Bob and sends a trap transaction
// 	const trapTx = AHApi.tx.PolkadotXcm.execute({
// 		message: alice_msg,
// 		max_weight: {
// 			ref_time: alice_weight.value.ref_time,
// 			proof_size: alice_weight.value.proof_size,
// 		},
// 	});

// 	const trapResult = await trapTx.signAndSubmit(aliceSigner);
// 	expect(trapResult.ok).toBeTruthy();

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

// 	// Transaction 2: Bob claims trapped assets.
// 	const bobClaimTx = AHApi.tx.PolkadotXcm.execute({
// 		message: bob_msg,
// 		max_weight: {
// 			ref_time: bob_weight.value.ref_time,
// 			proof_size: bob_weight.value.proof_size,
// 		},
// 	});

// 	const claimResult = await bobClaimTx.signAndSubmit(bobSigner);
// 	expect(claimResult.ok).toBeTruthy();

// 	const bobBalanceAfter = await getFreeBalance(AHApi, CONFIG.KEYS.BOB);
// 	expect(bobBalanceAfter > bobBalanceBefore).toBeTruthy();
// });

// test("Initiate Teleport XCM v4 (AH -> RC)", async () => {
// 	const bob_balance_before =  await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
// 	const deposit_amount = 5_000_000_000_000n;

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
// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);

// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});

// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();

// 	await rc_provider.send('dev_newBlock', [{ count: 1 }])
// 	const bob_balance_after = await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });

// test("Initiate Teleport XCM v5 (AH -> RC)", async () => {
// 	const bob_balance_before =  await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
// 	const deposit_amount = 1_000_000_000_000n;

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

// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});

// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	await rc_provider.send('dev_newBlock', [{ count: 1 }])
// 	expect(r).toBeTruthy();

// 	const bob_balance_after = await getFreeBalance(rcApi, CONFIG.KEYS.BOB);
// 	expect(bob_balance_after - bob_balance_before).toBe(deposit_amount);
// });

// test("Initiate Teleport (AH -> RC) with remote fees", async () => {
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

// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 		},
// 	);
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
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

// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: 100_000_000_000n, proof_size: 1_000_000n },
// 		},
// 	);
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
// });

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

// 	const penpalToAH = PenpalApi.tx.PolkadotXcm.execute({
// 			message: msg,
// 			max_weight: { ref_time: 100_000_000_000n, proof_size: 1_000_000n },
// 		},
// 	);
// 	const r = await penpalToAH.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
// });

// test("Teleport and Transact from Westend's Asset Hub to Penpal", async () => {
// 	const remarkWithEventCalldata = await PenpalApi.tx.System.remark_with_event({
// 		remark: Binary.fromText("Hello, World!"),
// 	}).getEncodedData();

// 	PenpalApi.event.System.Remarked.watch().forEach((e) =>
// 		console.log(
// 			`\nBlock: ${e.meta.block.number}\nSender: ${
// 				e.payload.sender
// 			}\nHash: ${e.payload.hash.asHex()}\n`
// 		)
// 	);

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

// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
// 	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
// 		msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
// 	const r = await ahToWnd.signAndSubmit(aliceSigner);
// 	expect(r.ok).toBeTruthy();
// });

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

// 	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
// 		message,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});
// 	const r = await ahToPpl.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();
// });

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

// 	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
// 	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
// 		message: msg,
// 		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
// 	});

// 	const r = await ahToPpl.signAndSubmit(aliceSigner);
// 	expect(r).toBeTruthy();

// 	expect(await getFreeBalance(AHApi, CONFIG.KEYS.ALICE)).toBeLessThan(
// 		1_000_000_000_000_000n - 10_000_000_000_000n
// 	);
// });

test(
	"Initiate Teleport and Transact(Balances.transfer) from Westend's Asset Hub to Westend People w/ InitiateTransfer",
	async () => {
		const amountForFeeOnSource = 2_000_000_000_000n;
		const amountForFeeOnRemote = amountForFeeOnSource;
		const amountToWithdrawOnSource = 10_000_000_000_000n;
		const amountToBeTransferredToBob = 1_000_000_000_000n;

		const transferCall =
			await peopleChainCtx.api.tx.Balances.transfer_allow_death({
				value: amountToBeTransferredToBob,
				dest: MultiAddress.Id(CONFIG.KEYS.BOB),
			}).getEncodedData();

		const aliceSovereignOnPeopleFromAssetHub = {
			parents: 1,
			interior: XcmV5Junctions.X2([
				XcmV5Junction.Parachain(1000),
				XcmV5Junction.AccountId32({
					network: undefined,
					id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
				}),
			]),
		};

		const msg = XcmVersionedXcm.V5([
			XcmV5Instruction.WithdrawAsset([
				{
					id: { parents: 1, interior: XcmV5Junctions.Here() },
					fun: XcmV3MultiassetFungibility.Fungible(amountToWithdrawOnSource),
				},
			]),
			XcmV5Instruction.PayFees({
				asset: {
					id: {
						parents: 1,
						interior: XcmV3Junctions.Here(),
					},
					fun: XcmV3MultiassetFungibility.Fungible(amountForFeeOnSource),
				},
			}),
			XcmV5Instruction.InitiateTransfer({
				destination: {
					parents: 1,
					interior: XcmV5Junctions.X1(XcmV5Junction.Parachain(1004)),
				},
				remote_fees: Enum(
					"Teleport",
					XcmV5AssetFilter.Definite([
						{
							id: {
								parents: 1,
								interior: XcmV5Junctions.Here(),
							},
							fun: XcmV3MultiassetFungibility.Fungible(amountForFeeOnRemote),
						},
					]),
				),
				preserve_origin: true,
				assets: [Enum("Teleport", XcmV5AssetFilter.Wild(XcmV5WildAsset.All()))],
				remote_xcm: [
					XcmV5Instruction.RefundSurplus(),
					XcmV5Instruction.DepositAsset({
						assets: XcmV5AssetFilter.Wild(XcmV5WildAsset.All()),
						beneficiary: aliceSovereignOnPeopleFromAssetHub,
					}),
					XcmV5Instruction.Transact({
						origin_kind: XcmV2OriginKind.SovereignAccount(),
						call: transferCall,
						fallback_max_weight: undefined,
					}),
					XcmV5Instruction.ExpectTransactStatus(XcmV3MaybeErrorCode.Success()),
				],
			}),
			XcmV5Instruction.RefundSurplus(),
			XcmV5Instruction.DepositAsset({
				assets: XcmV5AssetFilter.Wild(XcmV5WildAsset.All()),
				beneficiary: {
					parents: 0,
					interior: XcmV5Junctions.X1(
						XcmV5Junction.AccountId32({
							network: undefined,
							id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
						}),
					),
				},
			}),
		]);

		const weight =
			await assetHubChainCtx.api.apis.XcmPaymentApi.query_xcm_weight(msg);
		assert(weight.success);
		const ahToPpl = assetHubChainCtx.api.tx.PolkadotXcm.execute({
			message: msg,
			max_weight: {
				ref_time: weight.value.ref_time,
				proof_size: weight.value.proof_size,
			},
		});

		const aliceSaOnPeople =
			await peopleChainCtx.api.apis.LocationToAccountApi.convert_location(
				XcmVersionedLocation.V5(aliceSovereignOnPeopleFromAssetHub),
			);
		assert(aliceSaOnPeople.success);

		const aliceBalanceOnAssetHubBefore = await getFreeBalance(
			assetHubChainCtx.api,
			CONFIG.KEYS.ALICE,
		);
		const bobBalanceOnPeopleBefore = await getFreeBalance(
			peopleChainCtx.api,
			CONFIG.KEYS.BOB,
		);
		const aliceSaBalanceOnPeopleBefore = await getFreeBalance(
			peopleChainCtx.api,
			aliceSaOnPeople.value,
		);

		// wrapping signAndSubmit inside waitForFinalizedBlocks to make sure we've observed right amount of blocks finalized on target chain
		// before proceeding further
		const r = await waitForFinalizedBlocks(
			peopleChainCtx.client,
			1,
			async () => {
				return await ahToPpl.signAndSubmit(aliceSigner);
			},
		);
		assert(r.ok);

		const aliceBalanceOnAssetHubAfter = await getFreeBalance(
			assetHubChainCtx.api,
			CONFIG.KEYS.ALICE,
		);
		const bobBalanceOnPeopleAfter = await getFreeBalance(
			peopleChainCtx.api,
			CONFIG.KEYS.BOB,
		);
		const aliceSaBalanceOnPeopleAfter = await getFreeBalance(
			peopleChainCtx.api,
			aliceSaOnPeople.value,
		);

		const amountToTeleport = amountToWithdrawOnSource - amountForFeeOnSource;
		const feePaidOnSource =
			aliceBalanceOnAssetHubBefore -
			(aliceBalanceOnAssetHubAfter + amountToTeleport);
		const amountToDepositToRemoteOrigin =
			amountToTeleport - amountForFeeOnRemote;

		// verify that fee paid on source chain is actually greater than 0 but also within the amount intended for the fee
		expect(feePaidOnSource).toBeLessThanOrEqual(amountForFeeOnSource);
		expect(feePaidOnSource).not.toBeLessThanOrEqual(0);

		// verify that Alice's Sovereign Account on People chain received definite amount of teleported funds + some refunded after fees
		expect(aliceSaBalanceOnPeopleAfter).not.toBeLessThan(
			aliceSaBalanceOnPeopleBefore + amountToDepositToRemoteOrigin,
		);
		expect(aliceSaBalanceOnPeopleAfter).toBeLessThan(
			aliceSaBalanceOnPeopleBefore +
				amountToDepositToRemoteOrigin +
				amountForFeeOnRemote,
		);

		// confirm that Bob received funds delivered by means of a remote call Transact(Balances.transfer)
		expect(bobBalanceOnPeopleAfter).toBe(
			bobBalanceOnPeopleBefore + amountToBeTransferredToBob,
		);
	},
	{ timeout: 10000 },
);

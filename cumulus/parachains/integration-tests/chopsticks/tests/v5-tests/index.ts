import { test, expect } from "bun:test";
import {
	wnd_ah,
	wnd_penpal,
	XcmV3Junctions,
	XcmV3Junction,
	XcmV3MultiassetFungibility,
	XcmV4Instruction,
	XcmV3WeightLimit,
	XcmV3MultiassetMultiAssetFilter,
	XcmV4AssetAssetFilter,
	XcmV2OriginKind,
} from "@polkadot-api/descriptors";
import { Binary, Enum, createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import { DEV_PHRASE, entropyToMiniSecret, mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";

const WESTEND_NETWORK = Uint8Array.from([
	225, 67, 242, 56, 3, 172, 80, 232, 246, 248, 230, 38, 149, 209, 206, 158, 78, 29, 104, 170, 54,
	193, 205, 44, 253, 21, 52, 2, 19, 243, 66, 62,
]);
// TODO: find a way to extract keys below from yaml config.
const BOB_KEY = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
const ALICE_KEY = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

// Create and initialize clients
const ahClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:8000")));
const AHApi = ahClient.getTypedApi(wnd_ah);
const penaplClient = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:8001")));
const PenpalApi = penaplClient.getTypedApi(wnd_penpal);

// Initialize HDKD key pairs and signers
const entropy = mnemonicToEntropy(DEV_PHRASE);
const miniSecret = entropyToMiniSecret(entropy);
const derive = sr25519CreateDerive(miniSecret);

const hdkdKeyPairAlice = derive("//Alice");
const hdkdKeyPairBob = derive("//Bob");

const aliceSigner = getPolkadotSigner(hdkdKeyPairAlice.publicKey, "Sr25519", hdkdKeyPairAlice.sign);
const bobSigner = getPolkadotSigner(hdkdKeyPairBob.publicKey, "Sr25519", hdkdKeyPairBob.sign);

// Utility function for balance fetching
async function getFreeBalance(api, accountKey) {
	const balance = await api.query.System.Account.getValue(accountKey);
	return balance.data.free;
}

test("Set Asset Claimer, Trap Assets, Claim Trapped Assets", async () => {
	const bobBalanceBefore = await getFreeBalance(AHApi, BOB_KEY);

	const alice_msg = Enum("V5", [
		Enum("SetAssetClaimer", {
			location: {
				parents: 0,
				interior: XcmV3Junctions.X1(
					XcmV3Junction.AccountId32({
						network: Enum("ByGenesis", Binary.fromBytes(WESTEND_NETWORK)),
						id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
					})
				),
			},
		}),
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
			},
		]),
		XcmV4Instruction.ClearOrigin(),
	]);
	const alice_weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(alice_msg);

	// Transaction 1: Alice sets asset claimer to Bob and sends a trap transaction
	const trapTx = AHApi.tx.PolkadotXcm.execute({
		message: alice_msg,
		max_weight: {
			ref_time: alice_weight.value.ref_time,
			proof_size: alice_weight.value.proof_size,
		},
	});

	const trapResult = await trapTx.signAndSubmit(aliceSigner);
	expect(trapResult.ok).toBeTruthy();

	const bob_msg = Enum("V4", [
		XcmV4Instruction.ClaimAsset({
			assets: [
				{
					id: { parents: 1, interior: XcmV3Junctions.Here() },
					fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
				},
			],
			ticket: { parents: 0, interior: XcmV3Junctions.Here() },
		}),
		XcmV4Instruction.BuyExecution({
			fees: {
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000n),
			},
			weight_limit: XcmV3WeightLimit.Unlimited(),
		}),
		XcmV4Instruction.DepositAsset({
			assets: XcmV3MultiassetMultiAssetFilter.Definite([
				{
					fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
					id: { parents: 1, interior: XcmV3Junctions.Here() },
				},
			]),
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
	]);
	const bob_weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(bob_msg);

	// Transaction 2: Bob claims trapped assets.
	const bobClaimTx = AHApi.tx.PolkadotXcm.execute({
		message: bob_msg,
		max_weight: {
			ref_time: bob_weight.value.ref_time,
			proof_size: bob_weight.value.proof_size,
		},
	});

	const claimResult = await bobClaimTx.signAndSubmit(bobSigner);
	expect(claimResult.ok).toBeTruthy();

	const bobBalanceAfter = await getFreeBalance(AHApi, BOB_KEY);
	expect(bobBalanceAfter > bobBalanceBefore).toBeTruthy();
});

test("Initiate Teleport XCM v4", async () => {
	const msg = Enum("V4", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(7e12),
			},
		]),
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
});

test("Initiate Teleport XCM v5", async () => {
	const xcm_origin_msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
			},
		]),

		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
			},
		}),
		Enum("InitiateTransfer", {
			destination: {
				parents: 1,
				interior: XcmV3Junctions.Here(),
			},
			// optional field. an example of usage:
			// remote_fees: Enum('Teleport', {
			// 	type: 'Wild',
			// 	value: {
			// 		type: 'All',
			// 		value: undefined,
			// 	},
			// }),
			preserve_origin: false,
			assets: [
				Enum("Teleport", {
					type: "Wild",
					value: {
						type: "All",
						value: undefined,
					},
				}),
			],
			remote_xcm: [
				Enum("PayFees", {
					asset: {
						id: {
							parents: 0,
							interior: XcmV3Junctions.Here(),
						},
						fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
					},
				}),
				XcmV4Instruction.DepositAsset({
					assets: XcmV3MultiassetMultiAssetFilter.Definite([
						{
							fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
							id: { parents: 0, interior: XcmV3Junctions.Here() },
						},
					]),
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

	const xcm_origin_weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(xcm_origin_msg);
	const weight_to_asset_fee = await AHApi.apis.XcmPaymentApi.query_weight_to_asset_fee(
		{
			ref_time: xcm_origin_weight.value.ref_time,
			proof_size: xcm_origin_weight.value.proof_size,
		},
		Enum("V5", {
			parents: 1,
			interior: XcmV3Junctions.Here(),
		})
	);

	const msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(weight_to_asset_fee.value),
			},
		}),
		Enum("InitiateTransfer", {
			destination: {
				parents: 1,
				interior: XcmV3Junctions.Here(),
			},
			// optional field. an example of usage:
			// remote_fees: Enum('Teleport', {
			// 	type: 'Wild',
			// 	value: {
			// 		type: 'All',
			// 		value: undefined,
			// 	},
			// }),
			preserve_origin: false,
			assets: [
				Enum("Teleport", {
					type: "Wild",
					value: {
						type: "All",
						value: undefined,
					},
				}),
			],
			remote_xcm: [
				Enum("PayFees", {
					asset: {
						id: {
							parents: 0,
							interior: XcmV3Junctions.Here(),
						},
						fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
					},
				}),
				XcmV4Instruction.DepositAsset({
					assets: XcmV3MultiassetMultiAssetFilter.Definite([
						{
							fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
							id: { parents: 0, interior: XcmV3Junctions.Here() },
						},
					]),
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
});

test("Initiate Teleport with remote fees", async () => {
	const msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(2_000_000_000_000n),
			},
		}),
		Enum("InitiateTransfer", {
			destination: {
				parents: 1,
				interior: XcmV3Junctions.Here(),
			},
			// optional field. an example of usage:
			remote_fees: Enum("Teleport", {
				type: "Wild",
				value: {
					type: "All",
					value: undefined,
				},
			}),
			preserve_origin: false,
			assets: [
				Enum("Teleport", {
					type: "Wild",
					value: {
						type: "All",
						value: undefined,
					},
				}),
			],
			remote_xcm: [
				XcmV4Instruction.DepositAsset({
					assets: XcmV3MultiassetMultiAssetFilter.Wild({
						type: "All",
						value: undefined,
					}),
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
});

test("Local Reserve Asset Transfer of USDT from Asset Hub to Alice on Penpal", async () => {
	const msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
			},
		}),
		XcmV4Instruction.TransferReserveAsset({
			assets: [
				{
					id: {
						parents: 0,
						interior: XcmV3Junctions.X2([
							XcmV3Junction.PalletInstance(50),
							XcmV3Junction.GeneralIndex(1984n),
						]),
					},
					fun: XcmV3MultiassetFungibility.Fungible(100_000_000n),
				},
				{
					id: {
						parents: 1,
						interior: XcmV3Junctions.Here(),
					},
					fun: XcmV3MultiassetFungibility.Fungible(4_000_000_000_000n),
				},
			],
			dest: {
				parents: 1,
				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2042)),
			},
			xcm: [
				Enum("PayFees", {
					asset: {
						id: {
							parents: 1,
							interior: XcmV3Junctions.Here(),
						},
						fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000n),
					},
				}),
				XcmV4Instruction.DepositAsset({
					// some WND might get trapped bc of extra fungibles in PayFees above ^
					assets: XcmV3MultiassetMultiAssetFilter.Wild({
						type: "All",
						value: undefined,
					}),

					// ===================== GRANULAR VERSIONS =====================
					// assets: XcmV4AssetAssetFilter.Definite([{
					// 	id: {
					// 		parents: 1,
					// 		interior: XcmV3Junctions.Here(),
					// 	},
					// 	fun: XcmV3MultiassetFungibility.Fungible(3_995_000_000_000n),
					// }]),
					//
					// assets: XcmV4AssetAssetFilter.Definite([{
					// 	id: {
					// 		parents: 1,
					// 		interior: XcmV3Junctions.X3([
					// 			XcmV3Junction.Parachain(1000),
					// 			XcmV3Junction.PalletInstance(50),
					// 			XcmV3Junction.GeneralIndex(1984n)]),
					// 	},
					// 	fun: XcmV3MultiassetFungibility.Fungible(100_000_000n),
					// }]),

					beneficiary: {
						parents: 0,
						interior: XcmV3Junctions.X1(
							XcmV3Junction.AccountId32({
								network: undefined,
								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
							})
						),
					},
				}),
			],
		}),
	]);

	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
		message: msg,
		max_weight: { ref_time: 81834380000n, proof_size: 823193n },
	});
	const r = await ahToWnd.signAndSubmit(aliceSigner);
	expect(r).toBeTruthy();
});

// this test scenario works together with the previous one.
// previous test serves as a set-up for this one.
test("InitiateReserveWithdraw USDT from Penpal to Asset Hub Bob", async () => {
	const msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(3_995_000_000_000n),
			},
			{
				id: {
					parents: 1,
					interior: XcmV3Junctions.X3([
						XcmV3Junction.Parachain(1000),
						XcmV3Junction.PalletInstance(50),
						XcmV3Junction.GeneralIndex(1984n),
					]),
				},
				fun: XcmV3MultiassetFungibility.Fungible(70_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
			},
		}),
		XcmV4Instruction.InitiateReserveWithdraw({
			assets: XcmV4AssetAssetFilter.Wild({
				type: "All",
				value: undefined,
			}),
			reserve: {
				parents: 1,
				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1000)),
			},
			xcm: [
				Enum("PayFees", {
					asset: {
						id: {
							parents: 1,
							interior: XcmV3Junctions.Here(),
						},
						fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
					},
				}),
				XcmV4Instruction.DepositAsset({
					assets: XcmV3MultiassetMultiAssetFilter.Wild({
						type: "All",
						value: undefined,
					}),
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

	const penpalToAH = PenpalApi.tx.PolkadotXcm.execute({
		message: msg,
		max_weight: { ref_time: 81834380000n, proof_size: 823193n },
	});
	const r = await penpalToAH.signAndSubmit(aliceSigner);
	expect(r).toBeTruthy();
});

test("Teleport and Transact from Westend's Asset Hub to Penpal", async () => {
	const remarkWithEventCalldata = await PenpalApi.tx.System.remark_with_event({
		remark: Binary.fromText("Hello, World!"),
	}).getEncodedData();

	PenpalApi.event.System.Remarked.watch().forEach((e) =>
		console.log(
			`\nBlock: ${e.meta.block.number}\nSender: ${
				e.payload.sender
			}\nHash: ${e.payload.hash.asHex()}\n`
		)
	);

	const msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(4_000_000_000_000n),
			},
		}),
		Enum("InitiateTransfer", {
			destination: {
				parents: 1,
				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2042)),
			},
			remote_fees: {
				type: "Teleport",
				value: {
					type: "Wild",
					value: {
						type: "All",
						value: undefined,
					},
				},
			},
			preserve_origin: true,
			assets: [],
			remote_xcm: [
				{
					type: "Transact",
					value: {
						origin_kind: XcmV2OriginKind.SovereignAccount(),
						call: remarkWithEventCalldata,
					},
				},
			],
		}),
	]);

	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
	const ahToWnd = AHApi.tx.PolkadotXcm.execute({
		msg,
		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
	});
	const r = await ahToWnd.signAndSubmit(aliceSigner);
	expect(r.ok).toBeTruthy();
});

test("Initiate Teleport XCM v5 from Westend's Asset Hub to Westend People w/ InitiateTeleport", async () => {
	const message = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(5_000_000_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
			},
		}),
		XcmV4Instruction.InitiateTeleport({
			assets: XcmV4AssetAssetFilter.Wild({ type: "All", value: undefined }),
			dest: {
				parents: 1,
				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1004)),
			},
			xcm: [
				XcmV4Instruction.BuyExecution({
					fees: {
						id: { parents: 1, interior: XcmV3Junctions.Here() },
						fun: XcmV3MultiassetFungibility.Fungible(500_000_000_000n),
					},
					weight_limit: XcmV3WeightLimit.Unlimited(),
				}),
				XcmV4Instruction.DepositAsset({
					assets: XcmV4AssetAssetFilter.Wild({ type: "All", value: undefined }),
					beneficiary: {
						parents: 0,
						interior: XcmV3Junctions.X1(
							XcmV3Junction.AccountId32({
								network: undefined,
								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
							})
						),
					},
				}),
			],
		}),
	]);
	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(message);

	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
		message,
		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
	});
	const r = await ahToPpl.signAndSubmit(aliceSigner);
	expect(r).toBeTruthy();
});

test("Initiate Teleport XCM v5 from Westend's Asset Hub to Westend People w/ InitiateTransfer", async () => {
	const msg = Enum("V5", [
		XcmV4Instruction.WithdrawAsset([
			{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(10_000_000_000_000n),
			},
		]),
		Enum("PayFees", {
			asset: {
				id: {
					parents: 1,
					interior: XcmV3Junctions.Here(),
				},
				fun: XcmV3MultiassetFungibility.Fungible(665_000_000_000n),
			},
		}),
		Enum("InitiateTransfer", {
			destination: {
				parents: 1,
				interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1004)),
			},
			remote_fees: {
				type: "Teleport",
				value: {
					type: "Definite",
					value: [
						{
							id: {
								parents: 1,
								interior: XcmV3Junctions.Here(),
							},
							fun: XcmV3MultiassetFungibility.Fungible(365_000_000_000n),
						},
					],
				},
			},
			preserve_origin: true,
			assets: [
				Enum("Teleport", {
					type: "Wild",
					value: {
						type: "All",
						value: undefined,
					},
				}),
			],
			remote_xcm: [
				XcmV4Instruction.DepositAsset({
					assets: XcmV4AssetAssetFilter.Wild({
						type: "All",
						value: undefined,
					}),
					beneficiary: {
						parents: 0,
						interior: XcmV3Junctions.X1(
							XcmV3Junction.AccountId32({
								network: undefined,
								id: Binary.fromBytes(hdkdKeyPairAlice.publicKey),
							})
						),
					},
				}),
			],
		}),
	]);

	const weight = await AHApi.apis.XcmPaymentApi.query_xcm_weight(msg);
	const ahToPpl = AHApi.tx.PolkadotXcm.execute({
		message: msg,
		max_weight: { ref_time: weight.value.ref_time, proof_size: weight.value.proof_size },
	});

	const r = await ahToPpl.signAndSubmit(aliceSigner);
	expect(r).toBeTruthy();

	expect(await getFreeBalance(AHApi, ALICE_KEY)).toBeLessThan(
		1_000_000_000_000_000n - 10_000_000_000_000n
	);
});

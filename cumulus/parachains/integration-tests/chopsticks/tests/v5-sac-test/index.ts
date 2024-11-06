import { test, expect } from "bun:test";
import {
	wnd_ah,
	XcmV3Junctions,
	XcmV3Junction,
	XcmV3MultiassetFungibility,
	XcmV4Instruction,
	XcmV3WeightLimit,
	XcmV3MultiassetMultiAssetFilter
} from "@polkadot-api/descriptors";
import { Binary, Enum, createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { sr25519CreateDerive } from "@polkadot-labs/hdkd";
import {
	DEV_PHRASE,
	entropyToMiniSecret,
	mnemonicToEntropy,
} from "@polkadot-labs/hdkd-helpers";
import { getPolkadotSigner } from "polkadot-api/signer";

const WESTEND_NETWORK = Uint8Array.from([225, 67, 242, 56, 3, 172, 80, 232, 246, 248, 230, 38, 149, 209, 206, 158, 78, 29, 104, 170, 54, 193, 205, 44, 253, 21, 52, 2, 19, 243, 66, 62]);
// TODO: find a way to extract keys below from yaml config.
const BOB_KEY = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
const ALICE_KEY = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

// Create and initialize client
const client = createClient(withPolkadotSdkCompat(getWsProvider("ws://localhost:8000")));
const AHApi = client.getTypedApi(wnd_ah);

// Initialize HDKD key pairs and signers
const entropy = mnemonicToEntropy(DEV_PHRASE);
const miniSecret = entropyToMiniSecret(entropy);
const derive = sr25519CreateDerive(miniSecret);

const hdkdKeyPairAlice = derive("//Alice");
const hdkdKeyPairBob = derive("//Bob");

const aliceSigner = getPolkadotSigner(
	hdkdKeyPairAlice.publicKey,
	"Sr25519",
	hdkdKeyPairAlice.sign,
);

const bobSigner = getPolkadotSigner(
	hdkdKeyPairBob.publicKey,
	"Sr25519",
	hdkdKeyPairBob.sign,
);

// Utility function for balance fetching
async function getFreeBalance(api, accountKey) {
	const balance = await api.query.System.Account.getValue(accountKey);
	return balance.data.free;
}

test("Set Asset Claimer, Trap Assets, Claim Trapped Assets", async () => {
	const bobBalanceBefore = await getFreeBalance(AHApi, BOB_KEY);

	// Transaction 1: Alice sets asset claimer to Bob and sends a trap transaction
	const trapTx = AHApi.tx.PolkadotXcm.execute({
		message: Enum("V5", [
			Enum("SetAssetClaimer", {
				location: {
					parents: 0,
					interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
						network: Enum("ByGenesis", Binary.fromBytes(WESTEND_NETWORK)),
						id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
					}))
				},
			}),
			XcmV4Instruction.WithdrawAsset([{
				id: { parents: 1, interior: XcmV3Junctions.Here() },
				fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
			}]),
			XcmV4Instruction.ClearOrigin(),
		]),
		max_weight: { ref_time: 100_000_000_000n, proof_size: 300_000n },
	});

	const trapResult = await trapTx.signAndSubmit(aliceSigner);
	expect(trapResult.ok).toBeTruthy();

	// Transaction 2: Bob claims trapped assets.
	const bobClaimTx = AHApi.tx.PolkadotXcm.execute({
		message: Enum("V4", [
			XcmV4Instruction.ClaimAsset({
				assets: [{
					id: { parents: 1, interior: XcmV3Junctions.Here() },
					fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
				}],
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
				assets: XcmV3MultiassetMultiAssetFilter.Definite([{
					fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n),
					id: { parents: 1, interior: XcmV3Junctions.Here() },
				}]),
				beneficiary: {
					parents: 0,
					interior: XcmV3Junctions.X1(XcmV3Junction.AccountId32({
						network: undefined,
						id: Binary.fromBytes(hdkdKeyPairBob.publicKey),
					}))
				}
			}),
		]),
		max_weight: { ref_time: 100_000_000_000n, proof_size: 300_000n },
	});

	const claimResult = await bobClaimTx.signAndSubmit(bobSigner);
	expect(claimResult.ok).toBeTruthy();


	const bobBalanceAfter = await getFreeBalance(AHApi, BOB_KEY);
	expect(bobBalanceAfter > bobBalanceBefore).toBeTruthy();
});

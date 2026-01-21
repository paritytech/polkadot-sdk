import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import {
	aliceStashSigner,
	accountIdBytes,
	createOwnershipProof,
	encodeSessionKeys,
	generateSessionKeys,
	getApis,
	GlobalTimeout,
	logger,
} from "../src/utils";
import { Binary } from "polkadot-api";

// Use real-s preset where alice and bob are actual validators.
const PRESET: Presets = Presets.RealS;

test(
	`session keys set_keys and purge_keys flow on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();

		const sessionKeys = await generateSessionKeys("//Alice//stash");
		logger.info("Generated session keys for alice");

		const aliceStashBytes = accountIdBytes("//Alice//stash");
		logger.info(`Alice stash account: ${Buffer.from(aliceStashBytes).toString("hex")}`);

		const encodedKeys = encodeSessionKeys(sessionKeys);
		logger.info(`Encoded session keys: ${encodedKeys.length} bytes`);

		const proof = createOwnershipProof(sessionKeys, aliceStashBytes);
		logger.info(`Ownership proof: ${proof.length} bytes`);

		logger.info("Submitting set_keys for alice");
		const setKeysResult = await apis.paraApi.tx.StakingRcClient.set_keys({
			keys: Binary.fromBytes(encodedKeys),
			proof: Binary.fromBytes(proof),
			max_delivery_and_remote_execution_fee: undefined,
		}).signAndSubmit(aliceStashSigner);
		if (!setKeysResult.ok) {
			const errStr = JSON.stringify(setKeysResult, (_, v) =>
				typeof v === "bigint" ? v.toString() : v,
			);
			logger.error(`set_keys failed: ${errStr}`);
		} else {
			logger.info("set_keys succeeded");
		}

		const steps = [
			// Expect FeesPaid event after set_keys
			Observe.on(Chain.Parachain, "StakingRcClient", "FeesPaid")
				.withDataCheck((x: any) => {
					logger.info(`FeesPaid (set_keys): who=${x.who}, fees=${x.fees}`);
					return true;
				})
				.onPass(async () => {
					// After set_keys succeeds, submit purge_keys
					logger.info("Submitting purge_keys for alice");
					apis.paraApi.tx.StakingRcClient.purge_keys({
						max_delivery_and_remote_execution_fee: undefined,
					}).signAndSubmit(aliceStashSigner);
				}),
			// Expect second FeesPaid event after purge_keys
			Observe.on(Chain.Parachain, "StakingRcClient", "FeesPaid").withDataCheck((x: any) => {
				logger.info(`FeesPaid (purge_keys): who=${x.who}, fees=${x.fees}`);
				return true;
			}),
		];

		const testCase = new TestCase(
			steps.map((s) => s.build()),
			true,
			() => {
				killZn();
			},
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout },
);

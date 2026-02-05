import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import {
	aliceStashSigner,
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

		const encodedKeys = encodeSessionKeys(sessionKeys);
		logger.info(`Encoded session keys: ${encodedKeys.length} bytes`);

		logger.info("Submitting set_keys for alice");
		const setKeysResult = await apis.paraApi.tx.StakingRcClient.set_keys({
			keys: Binary.fromBytes(encodedKeys),
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
			// Expect FeesPaid event on AH after set_keys with non-zero fees
			Observe.on(Chain.Parachain, "StakingRcClient", "FeesPaid").withDataCheck((x: any) => {
				logger.info(`FeesPaid (set_keys): who=${x.who}, fees=${x.fees}`);
				// Verify fees are non-zero (delivery fees are charged)
				return BigInt(x.fees) > 0n;
			}),
			// Expect SessionKeysUpdated on RC confirming keys were set
			Observe.on(Chain.Relay, "StakingAhClient", "SessionKeysUpdated")
				.withDataCheck((x: any) => {
					logger.info(`SessionKeysUpdated: stash=${x.stash}, update=${x.update?.type}`);
					return x.update?.type === "Set";
				})
				.onPass(async () => {
					// After RC confirms keys set, submit purge_keys
					logger.info("Submitting purge_keys for alice");
					apis.paraApi.tx.StakingRcClient.purge_keys({
						max_delivery_and_remote_execution_fee: undefined,
					}).signAndSubmit(aliceStashSigner);
				}),
			// Expect FeesPaid event on AH after purge_keys with non-zero fees
			Observe.on(Chain.Parachain, "StakingRcClient", "FeesPaid").withDataCheck((x: any) => {
				logger.info(`FeesPaid (purge_keys): who=${x.who}, fees=${x.fees}`);
				// Verify fees are non-zero (delivery fees are charged)
				return BigInt(x.fees) > 0n;
			}),
			// Expect SessionKeysUpdated on RC confirming keys were purged
			Observe.on(Chain.Relay, "StakingAhClient", "SessionKeysUpdated").withDataCheck(
				(x: any) => {
					logger.info(`SessionKeysUpdated: stash=${x.stash}, update=${x.update?.type}`);
					return x.update?.type === "Purged";
				},
			),
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

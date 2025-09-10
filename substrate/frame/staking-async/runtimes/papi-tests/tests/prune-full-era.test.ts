import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched, spawnMiner } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { getApis, GlobalTimeout, logger, nullifyUnsigned } from "../src/utils";
import { commonSignedSteps } from "./common";

const PRESET: Presets = Presets.FakeDot;

test(
	`pruning era with signed (full solution) on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();
		const killMiner = await spawnMiner();

		// This test has no real assertions. Change the `HistoryDepth` to 1 in the runtime, run it,
		// and observe the logs and PoV sizes.
		const steps = [
			// first relay session change at block 11
			Observe.on(Chain.Relay, "Session", "NewSession").byBlock(11)
				.onPass(() => {
					nullifyUnsigned(apis.paraApi).then((ok) => {
						logger.verbose("Nullified signed phase:", ok);
					});
				}),
			Observe.on(Chain.Parachain, "Staking", "EraPruned")
				.withDataCheck((x) => x.index == 0),
			Observe.on(Chain.Parachain, "Staking", "EraPruned")
				.withDataCheck((x) => x.index == 1),
			// Observe.on(Chain.Parachain, "Staking", "EraPruned")
			// 	.withDataCheck((x) => x.index == 2),
			// Observe.on(Chain.Parachain, "Staking", "EraPruned")
			// 	.withDataCheck((x) => x.index == 3),
			// Observe.on(Chain.Parachain, "Staking", "EraPruned")
			// 	.withDataCheck((x) => x.index == 4),
		].map((s) => s.build())

		const testCase = new TestCase(
			steps,
			true,
			() => {
				killMiner();
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout * 10 }
);

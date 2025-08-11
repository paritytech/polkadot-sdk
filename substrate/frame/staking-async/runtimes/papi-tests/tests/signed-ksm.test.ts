import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched, spawnMiner } from "../src/cmd";
import { EventOutcome, runTest, TestCase } from "../src/test-case";
import { getApis, GlobalTimeout } from "../src/utils";
import { commonSignedSteps } from "./common";

const PRESET: Presets = Presets.FakeKsm;

test(
	`signed solution on ${PRESET}`,
	async () => {
		const { killZn, paraLog }  = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();
		const killMiner = await spawnMiner();

		const testCase = new TestCase(
			commonSignedSteps(16, 1000, apis),
			true,
			() => {
				killMiner();
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

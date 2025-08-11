import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { EventOutcome, runTest, TestCase } from "../src/test-case";
import { getApis, GlobalTimeout} from "../src/utils";
import { commonUnsignedSteps } from "./common";

const PRESET: Presets = Presets.FakeDev;

test(
	`unsigned solution on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);

		const apis = await getApis();
		const steps = commonUnsignedSteps(10, 4, 4, false, apis);

		const testCase = new TestCase(steps, true, () => {
			killZn();
		});

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

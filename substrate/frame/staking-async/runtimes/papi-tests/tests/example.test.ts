import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { getApis, GlobalTimeout, logger, nullifySigned } from "../src/utils";

/// This is the preset against which your test will run. See the README or `PResets` for more info.
const PRESET: Presets = Presets.FakeKsm;

test(
	`example test with preset ${PRESET}`,
	async () => {
		/// We run the test with our defined preset.
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		/// Grab PAPI Apis to both relay and parachain instance of the ZN.
		const apis = await getApis();

		// Our test is defined here. We expect a sequence of events to be observed in RC or
		// Parachain. The events that we can observe are defined in `test-case.ts`'s `runTest`. In
		// short, they are all of the events related to staking.
		const testCase = new TestCase(
			[
				Observe.on(Chain.Relay, "Session", "NewSession")
				// An event can be expected to happen by a certain block
				.byBlock(11)
				// And it can execute a callback when it passes.
				.onPass(() => {
					logger.verbose("New session observed on relay chain");
				})
				// and we can check the data of the event.
				.withDataCheck((x: any) => {
					logger.verbose("shall we check the data? maybe", x);
					return true
				}),
				// add more `Observe`s here
			].map((s) => s.build()),
			// Passing this to true will allow events to be _interleaved_. If set to `false`, the
			// above sequence of events are expected to happen in a strict order. If `true`, the
			// events of each `Chain` must happen in a strict order, but intra-chain events can come
			// in any order. For example, assume we have the following 4 observes in our test case:
			// 1. Observe.on(Chain.Relay, "Module", "Event1")
			// 2. Observe.on(Chain.Relay, "Module", "Event2")
			// 3. Observe.on(Chain.Para, "Module", "Event3")
			// 4. Observe.on(Chain.Para, "Module", "Event4")
			//
			// Without interleaving, 1 -> 4 has to be observed as-is.
			//
			// With interleaving, at any point in time, the first unobserved event of each chain
			// type is acceptable. For example, the following is valid:
			//
			// 3 -> 1 -> 4 -> 2
			//
			// In some sense, with `interleave = true`, we break apart the test case into two stacks
			// that need to be popped in order, while in interleave = false, it is one stack.
			true,
			// Something to happen when the test is over. Always kill ZN, and any other processes
			// you might spawn.
			() => {
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

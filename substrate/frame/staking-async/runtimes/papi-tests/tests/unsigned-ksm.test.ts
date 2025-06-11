import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { getApis, GlobalTimeout, logger, nullifySigned } from "../src/utils";

test(
	`unsigned solution on ${Presets.FakeKsm}`,
	async () => {
		const killHandle = await runPresetUntilLaunched(Presets.FakeKsm);
		const apis = await getApis();

		let steps = [
			// first relay session change at block 11
			Observe.on(Chain.Relay, "Session", "NewSession").byBlock(11),
			// by block 10 we will plan a new era
			Observe.on(Chain.Parachain, "Staking", "SessionRotated")
				.withDataCheck((x: any) => x.active_era == 0 && x.planned_era == 1)
				.byBlock(30)
				.onPass(() => {
					nullifySigned(apis.paraApi).then((ok) => {
						logger.verbose("Nullified signed phase:", ok);
					});
				}),
			// eventually we will verify a 4 page solution only
			Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Verified"),
			Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Verified"),
			Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Verified"),
			Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Verified"),
			// eventually it will be queued
			Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Queued"),
			// eventually multiblock election will transition to `Done`
			Observe.on(Chain.Parachain, "MultiBlockElection", "PhaseTransitioned").withDataCheck(
				(x: any) => x.to.type === "Done"
			),
		]

		const exports = Array.from({ length: 16 }, (_, __) => {
			return Observe.on(Chain.Parachain, "Staking", "PagedElectionProceeded")
		});
		steps = steps.concat(exports);

		const rest = [
			// eventually multiblock goes back to `Off`
			Observe.on(Chain.Parachain, "MultiBlockElection", "PhaseTransitioned").withDataCheck(
				(x: any) => x.to.type === "Off"
			),
			// eventually we will send it back to RC
			Observe.on(Chain.Relay, "StakingAhClient", "ValidatorSetReceived").withDataCheck(
				(x: any) => x.id === 1 && x.new_validator_set_count === 1000
			),
			Observe.on(Chain.Relay, "Session", "NewQueued"),
			// eventually we will receive a session report back in AH with activation timestamp
			Observe.on(Chain.Parachain, "StakingRcClient", "SessionReportReceived").withDataCheck(
				(x) => x.activation_timestamp !== undefined
			),
			// eventually we will have era paid
			Observe.on(Chain.Parachain, "Staking", "EraPaid"),
		]
		steps = steps.concat(rest);

		const testCase = new TestCase(
			steps.map((s) => s.build()),
			true,
			() => {
				// test runner will run these upon completion.
				killHandle();
			}
		);

		const outcome = await runTest(testCase, apis);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

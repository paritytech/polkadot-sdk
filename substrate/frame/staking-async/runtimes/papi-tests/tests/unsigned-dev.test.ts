import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, Observe, runTest, TestCase } from "../src/test-case";
import { GlobalTimeout } from "../src/utils";

test(`unsigned solution on ${Presets.FakeDev}`, async () => {
	const killHandle = await runPresetUntilLaunched(Presets.FakeDev);
	const testCase = new TestCase(
		[
			// first relay session change at block 11
			new Observe(Chain.Relay, "Session", "NewSession", undefined, 11),
			// by block 10 we will plan a new era
			new Observe(
				Chain.Parachain,
				"Staking",
				"SessionRotated",
				(x: any) => x.active_era == 0 && x.planned_era == 1,
				30
			),
			// eventually we will verify a 4 page solution
			new Observe(Chain.Parachain, "MultiBlockVerifier", "Verified"),
			new Observe(Chain.Parachain, "MultiBlockVerifier", "Verified"),
			new Observe(Chain.Parachain, "MultiBlockVerifier", "Verified"),
			new Observe(Chain.Parachain, "MultiBlockVerifier", "Verified"),
			new Observe(Chain.Parachain, "MultiBlockVerifier", "Queued"),
			// eventually we will export all 4 pages to staking
			// new Observe(Chain.Parachain, "Staking", "PagedElectionProceeded"),
			// new Observe(Chain.Parachain, "Staking", "PagedElectionProceeded"),
			// new Observe(Chain.Parachain, "Staking", "PagedElectionProceeded"),
			// new Observe(Chain.Parachain, "Staking", "PagedElectionProceeded"),
			// eventually multiblock goes back to `Off`
			// new Observe(
			// 	Chain.Parachain,
			// 	"MultiBlock",
			// 	"PhaseTransitioned",
			// 	(x: any) => x.to.type === "Off"
			// ),
			// eventually we will send it back to RC
			new Observe(
				Chain.Relay,
				"StakingAhClient",
				"ValidatorSetReceived",
				(x: any) => x.id === 1 && x.new_validator_set_count === 2
			),
			new Observe(Chain.Relay, "Session", "NewQeued"),
			// eventually we will receive a session report back in AH with activation timestamp
			new Observe(
				Chain.Parachain,
				"StakingRcClient",
				"SessionReportReceived",
				(x) => x.activation_timestamp !== undefined
			),
			// eventually we will have era paid
			new Observe(Chain.Parachain, "Staking", "EraPaid", undefined, 200),
		],
		() => {
			killHandle();
			Promise.resolve();
		}
	);

	await runTest(testCase);
}, {timeout: GlobalTimeout});

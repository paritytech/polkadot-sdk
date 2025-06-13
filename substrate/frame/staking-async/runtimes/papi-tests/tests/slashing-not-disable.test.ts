import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { alice, getApis, GlobalTimeout, logger } from "../src/utils";

test(
	`slashing without disabling on ${Presets.RealS}`,
	async () => {
		const killHandle = await runPresetUntilLaunched(Presets.RealS);
		const apis = await getApis();

		const steps = [
			// first relay session change at block 11
			Observe.on(Chain.Relay, "Session", "NewSession").byBlock(11),
			// eventually we will receive a session report back in AH with activation timestamp
			// at this point we are ready to submit a slash tx
			Observe.on(Chain.Parachain, "Staking", "SessionRotated")
				.byBlock(30)
				.withDataCheck((x: any) => x.active_era == 0 && x.planned_era == 1),
			// Eventually we will receive an activation timestamp.
			Observe.on(Chain.Parachain, "StakingRcClient", "SessionReportReceived")
				.withDataCheck((x) => x.activation_timestamp !== undefined)
				.onPass(() => {
					// upon completion, submit a slash to rc
					logger.info("Submitting slash to RC");
					const call = apis.rcApi.tx.RootOffences.create_offence({
						offenders: [
							// alice//Stash, 10%, which will not cause any disabling
							["5GNJqTPyNqANBkUVMN1LPPrxXnFouWXoe2wNSmmEoLctxiZY", 100000000],
						],
						maybe_identifications: undefined,
						maybe_session_index: undefined,
					}).decodedCall;
					apis.rcApi.tx.Sudo.sudo({ call })
						.signAndSubmit(alice)
						.then((res) => {
							logger.verbose("Slash submission result:", res.ok);
						});
				}),
			// we will receive the root offence event
			Observe.on(Chain.Relay, "RootOffences", "OffenceCreated"),
			// eventually we will receive an offence in the parachain
			Observe.on(Chain.Parachain, "StakingRcClient", "OffenceReceived"),
			// TODO: Ankan
		];
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

import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { alice, getApis, GlobalTimeout, logger, nullifySigned, aliceStash } from "../src/utils";

const PRESET: Presets = Presets.RealS;

test(
	`slashing without disabling on ${PRESET}`,
	async () => {
		const { killZn, paraLog }  = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();
		let aliceExposedNominators = 0;
		let pages = 0;

		const steps = [
			// first relay session change at block 11
			Observe.on(Chain.Relay, "Session", "NewSession").byBlock(11),
			// eventually AH will will be instructed to plan a new session.
			Observe.on(Chain.Parachain, "Staking", "SessionRotated")
				.withDataCheck((x: any) => x.active_era == 0 && x.planned_era == 1)
				.onPass(() => {
					nullifySigned(apis.paraApi);
				}),
			// Eventually we will receive an activation timestamp in AH, meaning the first era was complete.
			Observe.on(Chain.Parachain, "StakingRcClient", "SessionReportReceived")
				.withDataCheck((x) => x.activation_timestamp !== undefined)
				.onPass(() => {
					// upon completion, submit a slash to rc
					logger.info("Submitting slash to RC");
					const call = apis.rcApi.tx.RootOffences.create_offence({
						offenders: [
							// alice//Stash, 10%, which will NOT cause any disabling. See `DisablingStrategy` in `./runtimes/parachain`.
							[aliceStash, 100000000],
						],
						maybe_identifications: undefined,
						maybe_session_index: undefined,
					}).decodedCall;
					apis.rcApi.tx.Sudo.sudo({ call })
						.signAndSubmit(alice)
						.then((res: any) => {
							logger.verbose("Slash submission result:", res.ok);
						});
				}),
			// we will receive the root offence event
			Observe.on(Chain.Relay, "RootOffences", "OffenceCreated"),

			// eventually we will receive an offence in the parachain, first the rc-client
			Observe.on(Chain.Parachain, "StakingRcClient", "OffenceReceived").withDataCheck(
				(x: any) => x.offences_count === 1
			),
			// then staking
			Observe.on(Chain.Parachain, "Staking", "OffenceReported")
				.withDataCheck((x: any) => x.offence_era === 1 && x.fraction === 100000000)
				.onPass(async () => {
					// let's calculate how many pages of exposure alice has -- this will impact the number of next events.
					const overview = await apis.paraApi.query.Staking.ErasStakersOverview.getValue(
						1,
						aliceStash
					);
					pages = overview?.page_count || 0;
					aliceExposedNominators = overview?.nominator_count || 0;
					// TODO: lazily create the `Slashed` and `SlashComputed` based on this
					logger.verbose(
						`Alice has ${aliceExposedNominators} exposed nominators (${pages}) whom we expect to slash later`
					);
				}),

			// then staking will calculate the slasheh, we only check 1 page
			Observe.on(Chain.Parachain, "Staking", "SlashComputed").withDataCheck(
				(x: any) => x.page === 0
			),

			// staking will eventually bump to active era 2, where slashes will be applied.
			Observe.on(Chain.Parachain, "Staking", "EraPaid"),
			Observe.on(Chain.Parachain, "Staking", "SessionRotated").withDataCheck(
				(x: any) => x.active_era === 2
			),

			// staking will apply slashes, we only check one slash.
			Observe.on(Chain.Parachain, "Staking", "Slashed"),
		];
		const testCase = new TestCase(
			steps.map((s) => s.build()),
			true,
			() => {
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

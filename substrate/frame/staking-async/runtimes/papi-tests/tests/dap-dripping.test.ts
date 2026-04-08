import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { Binary } from "polkadot-api";
import { alice, getApis, GlobalTimeout, logger } from "../src/utils";

const PRESET: Presets = Presets.FakeDot;

/// Set budget allocation via sudo: 85% staker_rewards, 15% buffer.
/// Keys must be sorted (BTreeMap) and Perbill values must sum to 1_000_000_000.
async function setBudgetAllocation(apis: Awaited<ReturnType<typeof getApis>>): Promise<boolean> {
	const { paraApi } = apis;

	const call = paraApi.tx.Dap.set_budget_allocation({
		new_allocations: [
			[Binary.fromText("buffer"), 150_000_000],
			[Binary.fromText("staker_rewards"), 850_000_000],
		],
	}).decodedCall;

	const res = await paraApi.tx.Sudo.sudo({ call }).signAndSubmit(alice);
	return res.ok;
}

test(
	`DAP dripping and era reward snapshot on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();

		let budgetSet = false;
		let dripCount = 0;

		const checkDrip = (x: any) => {
			dripCount++;
			const minted = BigInt(x.total_minted);
			logger.info(`DAP drip #${dripCount}: total_minted=${minted}, elapsed=${x.elapsed_millis}ms`);
			return minted > 0n;
		};

		const testCase = new TestCase(
			[
				Observe.on(Chain.Relay, "Session", "NewSession")
					.byBlock(11)
					.onPass(() => {
						setBudgetAllocation(apis).then((ok) => {
							budgetSet = ok;
							logger.info(`Budget allocation set: ${ok}`);
						}).catch((err) => {
							logger.error(`setBudgetAllocation failed: ${err}`);
						});
					}),

				Observe.on(Chain.Parachain, "Dap", "BudgetAllocationUpdated"),

				// Two sequential drip observations to confirm continuous dripping.
				Observe.on(Chain.Parachain, "Dap", "IssuanceMinted").withDataCheck(checkDrip),
				Observe.on(Chain.Parachain, "Dap", "IssuanceMinted").withDataCheck(checkDrip),

				Observe.on(Chain.Parachain, "Staking", "SessionRotated"),

				// In DAP mode: remainder == 0 (era reward pre-funded by pot, not legacy inflation),
				// payout > 0 (transferred from era pot).
				Observe.on(Chain.Parachain, "Staking", "EraPaid")
					.withDataCheck((x: any) => {
						const payout = BigInt(x.validator_payout);
						const remainder = BigInt(x.remainder);
						logger.info(
							`EraPaid: era=${x.era_index}, validator_payout=${payout}, remainder=${remainder}`
						);
						return remainder === 0n && payout > 0n;
					}),
			].map((s) => s.build()),
			true,
			() => {
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(budgetSet).toBe(true);
		expect(dripCount).toBe(2);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

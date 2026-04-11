import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { getApis, GlobalTimeout, logger } from "../src/utils";

const PRESET: Presets = Presets.FakeDot;

test(
	`DAP dripping and era reward snapshot on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();

		let dripCount = 0;
		let perDripStakerReward = 0n;

		// DapIssuanceCadence = 60_000ms, parachain block time = 6_000ms.
		// elapsed should be cadence or cadence + one block at most.
		const CADENCE_MS = 60_000;
		const MAX_ELAPSED_MS = 72_000;

		const checkDrip = (x: any) => {
			dripCount++;
			const minted = BigInt(x.total_minted);
			const elapsed = Number(x.elapsed_millis);
			logger.info(`DAP drip #${dripCount}: total_minted=${minted}, elapsed=${elapsed}ms`);

			// Track 85% staker share from a "clean" drip (exactly at cadence).
			if (elapsed === CADENCE_MS && perDripStakerReward === 0n) {
				// 85% of total_minted goes to staker pot (Perbill::mul_floor).
				perDripStakerReward = (minted * 850_000_000n) / 1_000_000_000n;
			}

			return minted > 0n && elapsed >= CADENCE_MS && elapsed <= MAX_ELAPSED_MS;
		};

		const testCase = new TestCase(
			[
				// Budget allocation is now set at genesis, so dripping starts immediately.
				// Two sequential drip observations to confirm continuous dripping.
				Observe.on(Chain.Parachain, "Dap", "IssuanceMinted").withDataCheck(checkDrip),
				Observe.on(Chain.Parachain, "Dap", "IssuanceMinted").withDataCheck(checkDrip),

				Observe.on(Chain.Parachain, "Staking", "SessionRotated"),

				// In DAP mode: remainder == 0 (era reward pre-funded by pot, not legacy inflation),
				// payout > 0 (transferred from era pot).
				// payout should be multiple drips worth of staker rewards (85% of each drip).
				Observe.on(Chain.Parachain, "Staking", "EraPaid")
					.withDataCheck((x: any) => {
						const payout = BigInt(x.validator_payout);
						const remainder = BigInt(x.remainder);

						// Verify payout is a whole number of drips worth of staker rewards.
						// Rounding from Perbill::mul_floor means payout <= N * per_drip_total * 0.85.
						let dripMultiple = 0n;
						if (perDripStakerReward > 0n) {
							dripMultiple = payout / perDripStakerReward;
						}
						logger.info(
							`EraPaid: era=${x.era_index}, validator_payout=${payout}, remainder=${remainder}, ` +
							`~${dripMultiple} drips worth of staker rewards`
						);

						// Payout should represent multiple drips (era spans many blocks).
						return remainder === 0n && payout > 0n && dripMultiple >= 3n;
					}),
			].map((s) => s.build()),
			true,
			() => {
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(dripCount).toBe(2);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);

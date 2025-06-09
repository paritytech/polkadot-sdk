import { rcPresetFor, runPreset } from "./cmd";
import { logger } from "./utils";
import { Command } from "commander";

export enum Presets {
	FakeDev = "fake-dev",
	FakeDot = "fake-dot",
	FakeKsm = "fake-ksm",
	RealS = "real-s",
	RealM = "real-m",
}

if (require.main === module) {
	const program = new Command();
	program
		.name("staking-async-papi-tests")
		.description("Run staking-async PAPI tests")
		.version("0.1.0");

	program
		.command("run")
		.description("Run a given preset. This just sets up the ZN env and runs it")
		.option(
			"-p, --para-preset <preset>",
			"run the given parachain preset. The right relay preset, and zn-toml file are auto-chosen.",
			Presets.FakeDev
		)
		.action(async (options) => {
			const { paraPreset } = options;
			runPreset(paraPreset);
		});

	program.parse(process.argv);
}

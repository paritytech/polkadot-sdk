import { spawn, spawnSync } from "child_process";
import { Presets } from "./index";
import { logger } from "./utils";
import { join } from "path";
import stripAnsi from "strip-ansi";

export function rcPresetFor(paraPreset: Presets): string {
	return paraPreset == Presets.FakeDev ||
		paraPreset == Presets.FakeDot ||
		paraPreset == Presets.FakeKsm
		? "fake-s"
		: paraPreset;
}

export function znConfigFor(paraPreset: Presets): string {
	return paraPreset == Presets.RealM ? "../zn-m.toml" : "../zn-s.toml";
}


export async function runPreset(paraPreset: Presets): Promise<void> {
	prepPreset(paraPreset);
	const znConfig = znConfigFor(paraPreset);
	logger.info(`Launching ZN config for preset: ${paraPreset}, config: ${znConfig}`);
	cmd(
		"zombienet",
		["--provider", "native", "-l", "text", "spawn", znConfig],
		"inherit"
	);
}

export async function runPresetUntilLaunched(paraPreset: Presets): Promise<() => void> {
	prepPreset(paraPreset);
	const znConfig = znConfigFor(paraPreset);
	logger.info(`Launching ZN config for preset: ${paraPreset}, config: ${znConfig}`);
	const child = spawn(
		"zombienet",
		["--provider", "native", "-l", "text", "spawn", znConfig],
		{ stdio: "pipe", cwd: __dirname }
	);

	return new Promise<() => void>((resolve, reject) => {
		const logCmds: string[] = [];
		child.stdout.on("data", (data) => {
			const raw: string = stripAnsi(data.toString());
			if(raw.includes("Log Cmd : ")) {
				raw.split("\n").filter((line) => line.includes("Log Cmd : ")).forEach((line) => {
					logCmds.push(line.replace("Log Cmd : ", "").trim());
				});
			}
			// our hacky way to know ZN is done.
			if (raw.includes("Parachain ID : 1100")) {
				for (const cmd of logCmds) {
					logger.info(`${cmd}`);
				}
				logger.info(`Launched ZN: ${paraPreset}`);
				resolve(() => {
					child.kill();
					logger.info(`Killed ZN config: ${paraPreset}`);
				});
			}
		});

		child.on("error", (err) => {
			reject(err);
		});
	});
}

function prepPreset(paraPreset: Presets): void {
	const rcPreset = rcPresetFor(paraPreset);
	const targetDir = "../../../../../../target";

	logger.info(`Running para-preset: ${paraPreset}, rc-preset: ${rcPreset}`);
	cmd("cargo", [
		"build",
		"--release",
		`-p`,
		`pallet-staking-async-rc-runtime`,
		`-p`,
		`pallet-staking-async-parachain-runtime`,
		`-p`,
		`staging-chain-spec-builder`,
	]);

	cmd("rm", ["./parachain.json"]);
	cmd("rm", ["./rc.json"]);

	cmd(join(targetDir, "/release/chain-spec-builder"), [
		"create",
		"-t",
		"development",
		"--runtime",
		join(
			targetDir,
			"/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.compressed.wasm"
		),
		"--relay-chain",
		"rococo-local",
		"--para-id",
		"1100",
		"named-preset",
		paraPreset,
	]);
	cmd("mv", ["chain_spec.json", "parachain.json"]);

	cmd(join(targetDir, "/release/chain-spec-builder"), [
		"create",
		"-t",
		"development",
		"--runtime",
		join(
			targetDir,
			"/release/wbuild/pallet-staking-async-rc-runtime/fast_runtime_binary.rs.wasm"
		),
		"named-preset",
		rcPreset,
	]);
	cmd("mv", ["chain_spec.json", "rc.json"]);
}

function cmd(cmd: string, args: string[], stdio: string = "ignore"): void {
	logger.info(`Running command: ${cmd} ${args.join(" ")}`);
	// @ts-ignore
	const result = spawnSync(cmd, args, { stdio: stdio, cwd: __dirname });
	if (result.error) {
		logger.error(`Error running command: ${cmd} ${args.join(" ")}`, result.error);
		throw result.error;
	}
}

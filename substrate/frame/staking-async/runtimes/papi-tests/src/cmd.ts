import { spawn, spawnSync } from "child_process";
import { Presets } from "./index";
import { logger } from "./utils";
import { join } from "path";
import stripAnsi from "strip-ansi";
import { createWriteStream } from "fs";

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

/// Returns the parachain log file.
export async function runPreset(paraPreset: Presets): Promise<void> {
	prepPreset(paraPreset);
	const znConfig = znConfigFor(paraPreset);
	logger.info(`Launching ZN config for preset: ${paraPreset}, config: ${znConfig}`);
	cmd("zombienet", ["--provider", "native", "-l", "text", "spawn", znConfig], "inherit");
}

export async function runPresetUntilLaunched(
	paraPreset: Presets
): Promise<{ killZn: () => void; paraLog: string | null }> {
	prepPreset(paraPreset);
	const znConfig = znConfigFor(paraPreset);
	logger.info(`Launching ZN config for preset: ${paraPreset}, config: ${znConfig}`);
	const child = spawn("zombienet", ["--provider", "native", "-l", "text", "spawn", znConfig], {
		stdio: "pipe",
		cwd: __dirname,
	});

	return new Promise<{ killZn: () => void; paraLog: string | null }>((resolve, reject) => {
		const logCmds: string[] = [];
		child.stdout.on("data", (data) => {
			const raw: string = stripAnsi(data.toString());
			if (raw.includes("Log Cmd : ")) {
				raw.split("\n")
					.filter((line) => line.includes("Log Cmd : "))
					.forEach((line) => {
						logCmds.push(line.replace("Log Cmd : ", "").trim());
					});
			}
			// our hacky way to know ZN is done.
			if (raw.includes("Parachain ID : 1100")) {
				for (const cmd of logCmds) {
					logger.info(`${cmd}`);
				}
				logger.info(`Launched ZN: ${paraPreset}`);

				// Extract log path from the last log command
				const lastCmd = logCmds[logCmds.length - 1];
				const paraLog = lastCmd ? lastCmd.match(/tail -f\s+(.+\.log)/)?.[1] || null : null;

				resolve({
					killZn: () => {
						child.kill();
						logger.verbose(`Killed zn process`);
					},
					paraLog,
				});
			}
		});

		child.on("error", (err) => {
			reject(err);
		});
	});
}

export async function spawnMiner(): Promise<() => void> {
	logger.info(`Spawning miner in background`);

	const logFile = createWriteStream(join(__dirname, "miner.log"), { flags: "a" });

	const child = spawn(
		"polkadot-staking-miner",
		[
			"--uri",
			"ws://127.0.0.1:9946",
			"experimental-monitor-multi-block",
			"--seed-or-path",
			"//Bob",
		],
		{ stdio: "pipe", cwd: __dirname }
	);

	child.stdout?.pipe(logFile);
	child.stderr?.pipe(logFile);

	return new Promise<() => void>((resolve, reject) => {
		child.on("error", (err) => {
			logger.error(`Error in miner miner: ${err}`);
			reject(err);
		});
		resolve(() => {
			logger.verbose(`Killing miner process`);
			logFile.end();
			child.kill();
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

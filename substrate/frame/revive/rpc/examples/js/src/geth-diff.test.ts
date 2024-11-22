import { spawn, spawnSync, Subprocess } from 'bun'
import { join } from 'path'
import { readFileSync } from 'fs'
import { afterAll, afterEach, beforeAll, describe, expect, test } from 'bun:test'
import {
	createWalletClient,
	defineChain,
	encodeFunctionData,
	Hex,
	http,
	parseEther,
	publicActions,
} from 'viem'
import { privateKeyToAccount } from 'viem/accounts'
import { abi } from '../abi/errorTester'

export function getByteCode(name: string, evm: boolean): Hex {
	const bytecode = evm ? readFileSync(`evm/${name}.bin`) : readFileSync(`pvm/${name}.polkavm`)
	return `0x${Buffer.from(bytecode).toString('hex')}`
}

type JsonRpcError = {
	code: number
	message: string
	data: Hex
}

function killProcessOnPort(port: number) {
	// Check which process is using the specified port
	const result = spawnSync(['lsof', '-ti', `:${port}`])
	const output = result.stdout.toString().trim()

	if (output) {
		console.log(`Port ${port} is in use. Killing process...`)
		const pids = output.split('\n')

		// Kill each process using the port
		for (const pid of pids) {
			spawnSync(['kill', '-9', pid])
			console.log(`Killed process with PID: ${pid}`)
		}
	}
}

let jsonRpcErrors: JsonRpcError[] = []
async function createEnv(name: 'geth' | 'kitchensink') {
	const gethPort = process.env.GETH_PORT || '8546'
	const kitchensinkPort = process.env.KITCHENSINK_PORT || '8545'
	const url = `http://localhost:${name == 'geth' ? gethPort : kitchensinkPort}`
	const chain = defineChain({
		id: name == 'geth' ? 1337 : 420420420,
		name,
		nativeCurrency: {
			name: 'Westie',
			symbol: 'WST',
			decimals: 18,
		},
		rpcUrls: {
			default: {
				http: [url],
			},
		},
		testnet: true,
	})

	const transport = http(url, {
		onFetchResponse: async (response) => {
			const raw = await response.clone().json()
			if (raw.error) {
				jsonRpcErrors.push(raw.error as JsonRpcError)
			}
		},
	})

	const wallet = createWalletClient({
		transport,
		chain,
	})

	const [account] = await wallet.getAddresses()
	const serverWallet = createWalletClient({
		account,
		transport,
		chain,
	}).extend(publicActions)

	const accountWallet = createWalletClient({
		account: privateKeyToAccount(
			'0xa872f6cbd25a0e04a08b1e21098017a9e6194d101d75e13111f71410c59cd57f'
		),
		transport,
		chain,
	}).extend(publicActions)

	return { serverWallet, accountWallet, evm: name == 'geth' }
}

// wait for http request to return 200
export function waitForHealth(url: string) {
	return new Promise<void>((resolve, reject) => {
		const start = Date.now()
		const interval = setInterval(() => {
			fetch(url)
				.then((res) => {
					if (res.status === 200) {
						clearInterval(interval)
						resolve()
					}
				})
				.catch(() => {
					const elapsed = Date.now() - start
					if (elapsed > 30_000) {
						clearInterval(interval)
						reject(new Error('hit timeout'))
					}
				})
		}, 1000)
	})
}

const procs: Subprocess[] = []
if (!process.env.USE_LIVE_SERVERS) {
	procs.push(
		// Run geth on port 8546
		//
		(() => {
			killProcessOnPort(8546)
			return spawn(
				'geth --http --http.api web3,eth,debug,personal,net --http.port 8546 --dev --verbosity 0'.split(
					' '
				),
				{ stdout: Bun.file('/tmp/geth.out.log'), stderr: Bun.file('/tmp/geth.err.log') }
			)
		})(),
		//Run the substate node
		(() => {
		killProcessOnPort(9944)
		return spawn(
			[
				'./target/debug/substrate-node',
				'--dev',
				'-l=error,evm=debug,sc_rpc_server=info,runtime::revive=debug',
			],
			{
				stdout: Bun.file('/tmp/kitchensink.out.log'),
				stderr: Bun.file('/tmp/kitchensink.err.log'),
				cwd: join(process.env.HOME!, 'polkadot-sdk'),
			}
		)
		})()
		,
		// Run eth-rpc on 8545
		await (async () => {
			killProcessOnPort(8545)
			const proc = spawn(
				[
					'./target/debug/eth-rpc',
					'--dev',
					'--node-rpc-url=ws://localhost:9944',
					'-l=rpc-metrics=debug,eth-rpc=debug',
				],
				{
					stdout: Bun.file('/tmp/eth-rpc.out.log'),
					stderr: Bun.file('/tmp/eth-rpc.err.log'),
					cwd: join(process.env.HOME!, 'polkadot-sdk'),
				}
			)
			await waitForHealth('http://localhost:8545/health').catch()
			return proc
		})()
	)
}

afterEach(() => {
	jsonRpcErrors = []
})

afterAll(async () => {
	procs.forEach((proc) => proc.kill())
})

const envs = await Promise.all([createEnv('geth'), createEnv('kitchensink')])

for (const env of envs) {
	describe(env.serverWallet.chain.name, () => {
		let errorTesterAddr: Hex = '0x'
		beforeAll(async () => {
			const hash = await env.serverWallet.deployContract({
				abi,
				bytecode: getByteCode('errorTester', env.evm),
			})
			const deployReceipt = await env.serverWallet.waitForTransactionReceipt({ hash })
			if (!deployReceipt.contractAddress) throw new Error('Contract address should be set')
			errorTesterAddr = deployReceipt.contractAddress
		})

		test('triggerAssertError', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.readContract({
					address: errorTesterAddr,
					abi,
					functionName: 'triggerAssertError',
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x4e487b710000000000000000000000000000000000000000000000000000000000000001'
				)
				expect(lastJsonRpcError?.message).toBe('execution reverted: assert(false)')
			}
		})

		test('triggerRevertError', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.readContract({
					address: errorTesterAddr,
					abi,
					functionName: 'triggerRevertError',
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.message).toBe('execution reverted: This is a revert error')
				expect(lastJsonRpcError?.data).toBe(
					'0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001654686973206973206120726576657274206572726f7200000000000000000000'
				)
			}
		})

		test('triggerDivisionByZero', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.readContract({
					address: errorTesterAddr,
					abi,
					functionName: 'triggerDivisionByZero',
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x4e487b710000000000000000000000000000000000000000000000000000000000000012'
				)
				expect(lastJsonRpcError?.message).toBe(
					'execution reverted: division or modulo by zero'
				)
			}
		})

		test('triggerOutOfBoundsError', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.readContract({
					address: errorTesterAddr,
					abi,
					functionName: 'triggerOutOfBoundsError',
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x4e487b710000000000000000000000000000000000000000000000000000000000000032'
				)
				expect(lastJsonRpcError?.message).toBe(
					'execution reverted: out-of-bounds access of an array or bytesN'
				)
			}
		})

		test('triggerCustomError', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.readContract({
					address: errorTesterAddr,
					abi,
					functionName: 'triggerCustomError',
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x8d6ea8be0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001654686973206973206120637573746f6d206572726f7200000000000000000000'
				)
				expect(lastJsonRpcError?.message).toBe('execution reverted')
			}
		})

		test('eth_call (not enough funds)', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.simulateContract({
					address: errorTesterAddr,
					abi,
					functionName: 'valueMatch',
					value: parseEther('10'),
					args: [parseEther('10')],
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate (not enough funds)', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.estimateContractGas({
					address: errorTesterAddr,
					abi,
					functionName: 'valueMatch',
					value: parseEther('10'),
					args: [parseEther('10')],
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate (revert)', async () => {
			expect.assertions(3)
			try {
				await env.serverWallet.estimateContractGas({
					address: errorTesterAddr,
					abi,
					functionName: 'valueMatch',
					value: parseEther('11'),
					args: [parseEther('10')],
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.message).toBe(
					'execution reverted: msg.value does not match value'
				)
				expect(lastJsonRpcError?.data).toBe(
					'0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001e6d73672e76616c756520646f6573206e6f74206d617463682076616c75650000'
				)
			}
		})

		test('eth_get_balance (no account)', async () => {
			const balance = await env.serverWallet.getBalance({
				address: '0x0000000000000000000000000000000000000123',
			})
			expect(balance).toBe(0n)
		})

		test('eth_estimate (not enough funds to cover gas specified)', async () => {
			expect.assertions(4)
			try {
				let balance = await env.serverWallet.getBalance(env.accountWallet.account)
				expect(balance).toBe(0n)

				await env.accountWallet.estimateContractGas({
					address: errorTesterAddr,
					abi,
					functionName: 'setState',
					args: [true],
				})
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate (no gas specified)', async () => {
			let balance = await env.serverWallet.getBalance(env.accountWallet.account)
			expect(balance).toBe(0n)

			const data = encodeFunctionData({
				abi,
				functionName: 'setState',
				args: [true],
			})

			await env.accountWallet.request({
				method: 'eth_estimateGas',
				params: [
					{
						data,
						from: env.accountWallet.account.address,
						to: errorTesterAddr,
					},
				],
			})
		})
	})
}

import {
	jsonRpcErrors,
	createEnv,
	getByteCode,
	killProcessOnPort,
	waitForHealth,
	polkadotSdkPath,
	visit,
} from './util.ts'
import { afterAll, afterEach, describe, expect, test } from 'bun:test'
import { encodeFunctionData, Hex, parseEther, decodeEventLog } from 'viem'
import { ErrorsAbi } from '../abi/Errors'
import { EventExampleAbi } from '../abi/EventExample'
import { TracingCallerAbi } from '../abi/TracingCaller'
import { TracingCalleeAbi } from '../abi/TracingCallee'
import { Subprocess, spawn } from 'bun'
import { fail } from 'node:assert'

const procs: Subprocess[] = []
if (process.env.START_GETH) {
	process.env.USE_GETH = 'true'
	procs.push(
		// Run geth on port 8546
		await (async () => {
			killProcessOnPort(8546)
			console.log('Starting geth')
			const proc = spawn(
				'geth --http --http.api web3,eth,debug,personal,net --http.port 8546 --dev --verbosity 0'.split(
					' '
				),
				{ stdout: Bun.file('/tmp/geth.out.log'), stderr: Bun.file('/tmp/geth.err.log') }
			)

			await waitForHealth('http://localhost:8546').catch()
			return proc
		})()
	)
}

if (process.env.START_SUBSTRATE_NODE) {
	procs.push(
		//Run the substate node
		(() => {
			killProcessOnPort(9944)
			console.log('Starting substrate node')
			return spawn(
				[
					'./target/debug/substrate-node',
					'--dev',
					'-l=error,evm=debug,sc_rpc_server=info,runtime::revive=debug',
				],
				{
					stdout: Bun.file('/tmp/substrate-node.out.log'),
					stderr: Bun.file('/tmp/substrate-node.err.log'),
					cwd: polkadotSdkPath,
				}
			)
		})()
	)
}

if (process.env.START_ETH_RPC) {
	process.env.USE_ETH_RPC = 'true'
	// Run eth-rpc on 8545
	procs.push(
		await (async () => {
			killProcessOnPort(8545)
			console.log('Starting eth-rpc')
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
					cwd: polkadotSdkPath,
				}
			)
			await waitForHealth('http://localhost:8545').catch()
			return proc
		})()
	)
}

afterEach(() => {
	jsonRpcErrors.length = 0
})

afterAll(async () => {
	procs.forEach((proc) => proc.kill())
})

const envs = await Promise.all([
	...(process.env.USE_GETH ? [createEnv('geth')] : []),
	...(process.env.USE_ETH_RPC ? [createEnv('eth-rpc')] : []),
])

for (const env of envs) {
	describe(env.serverWallet.chain.name, () => {
		const getErrorTesterAddr = (() => {
			let contractAddress: Hex = '0x'
			return async () => {
				if (contractAddress !== '0x') {
					return contractAddress
				}
				const hash = await env.serverWallet.deployContract({
					abi: ErrorsAbi,
					bytecode: getByteCode('Errors', env.evm),
				})
				const deployReceipt = await env.serverWallet.waitForTransactionReceipt({ hash })
				contractAddress = deployReceipt.contractAddress!
				return contractAddress
			}
		})()

		const getEventExampleAddr = (() => {
			let contractAddress: Hex = '0x'
			return async () => {
				if (contractAddress !== '0x') {
					return contractAddress
				}
				const hash = await env.serverWallet.deployContract({
					abi: EventExampleAbi,
					bytecode: getByteCode('EventExample', env.evm),
				})
				const deployReceipt = await env.serverWallet.waitForTransactionReceipt({ hash })
				contractAddress = deployReceipt.contractAddress!
				return contractAddress
			}
		})()

		const getTracingExampleAddrs = (() => {
			let callerAddr: Hex = '0x'
			let calleeAddr: Hex = '0x'
			return async () => {
				if (callerAddr !== '0x') {
					return [callerAddr, calleeAddr]
				}
				calleeAddr = await (async () => {
					const hash = await env.serverWallet.deployContract({
						abi: TracingCalleeAbi,
						bytecode: getByteCode('TracingCallee', env.evm),
					})
					const receipt = await env.serverWallet.waitForTransactionReceipt({
						hash,
					})
					return receipt.contractAddress!
				})()

				callerAddr = await (async () => {
					const hash = await env.serverWallet.deployContract({
						abi: TracingCallerAbi,
						args: [calleeAddr],
						bytecode: getByteCode('TracingCaller', env.evm),
						value: parseEther('10'),
					})
					const receipt = await env.serverWallet.waitForTransactionReceipt({
						hash,
					})
					return receipt.contractAddress!
				})()

				return [callerAddr, calleeAddr]
			}
		})()

		test('triggerAssertError', async () => {
			try {
				await env.accountWallet.readContract({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'triggerAssertError',
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x4e487b710000000000000000000000000000000000000000000000000000000000000001'
				)
				expect(lastJsonRpcError?.message).toBeOneOf([
					'execution reverted: assert(false)',
					'execution reverted: panic: assertion failed (0x01)',
				])
			}
		})

		test('triggerRevertError', async () => {
			try {
				await env.accountWallet.readContract({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'triggerRevertError',
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.message).toBeOneOf([
					'execution reverted: This is a revert error',
					'execution reverted: revert: This is a revert error',
				])
				expect(lastJsonRpcError?.data).toBe(
					'0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001654686973206973206120726576657274206572726f7200000000000000000000'
				)
			}
		})

		test('triggerDivisionByZero', async () => {
			try {
				await env.accountWallet.readContract({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'triggerDivisionByZero',
				})
				expect.assertions(3)
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x4e487b710000000000000000000000000000000000000000000000000000000000000012'
				)
				expect(lastJsonRpcError?.message).toBeOneOf([
					'execution reverted: division or modulo by zero',
					'execution reverted: panic: division or modulo by zero (0x12)',
				])
			}
		})

		test('triggerOutOfBoundsError', async () => {
			try {
				await env.accountWallet.readContract({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'triggerOutOfBoundsError',
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.data).toBe(
					'0x4e487b710000000000000000000000000000000000000000000000000000000000000032'
				)
				expect(lastJsonRpcError?.message).toBeOneOf([
					'execution reverted: out-of-bounds access of an array or bytesN',
					'execution reverted: panic: array out-of-bounds access (0x32)',
				])
			}
		})

		test('triggerCustomError', async () => {
			try {
				await env.accountWallet.readContract({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'triggerCustomError',
				})
				fail('Expect call to fail')
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
			try {
				await env.emptyWallet.simulateContract({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'valueMatch',
					value: parseEther('10'),
					args: [parseEther('10')],
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_call transfer (not enough funds)', async () => {
			const value = parseEther('10')
			const balance = await env.emptyWallet.getBalance(env.emptyWallet.account)
			expect(balance, 'Balance should be less than 10').toBeLessThan(value)
			try {
				await env.emptyWallet.sendTransaction({
					to: '0x75E480dB528101a381Ce68544611C169Ad7EB342',
					value,
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate (not enough funds)', async () => {
			try {
				await env.emptyWallet.estimateContractGas({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'valueMatch',
					value: parseEther('10'),
					args: [parseEther('10')],
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate call caller (not enough funds)', async () => {
			try {
				await env.emptyWallet.estimateContractGas({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'valueMatch',
					value: parseEther('10'),
					args: [parseEther('10')],
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate (revert)', async () => {
			try {
				await env.serverWallet.estimateContractGas({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'valueMatch',
					value: parseEther('11'),
					args: [parseEther('10')],
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(3)
				expect(lastJsonRpcError?.message).toBeOneOf([
					'execution reverted: msg.value does not match value',
					'execution reverted: revert: msg.value does not match value',
				])
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
			let balance = await env.serverWallet.getBalance(env.emptyWallet.account)
			expect(balance).toBe(0n)
			try {
				await env.emptyWallet.estimateContractGas({
					address: await getErrorTesterAddr(),
					abi: ErrorsAbi,
					functionName: 'setState',
					args: [true],
				})
				fail('Expect call to fail')
			} catch (err) {
				const lastJsonRpcError = jsonRpcErrors.pop()
				expect(lastJsonRpcError?.code).toBe(-32000)
				expect(lastJsonRpcError?.message).toInclude('insufficient funds')
				expect(lastJsonRpcError?.data).toBeUndefined()
			}
		})

		test('eth_estimate (no gas specified)', async () => {
			let balance = await env.serverWallet.getBalance(env.emptyWallet.account)
			expect(balance).toBe(0n)

			const data = encodeFunctionData({
				abi: ErrorsAbi,
				functionName: 'setState',
				args: [true],
			})

			await env.emptyWallet.request({
				method: 'eth_estimateGas',
				params: [
					{
						data,
						from: env.emptyWallet.account.address,
						to: await getErrorTesterAddr(),
					},
				],
			})
		})

		test('logs', async () => {
			let address = await getEventExampleAddr()
			let { request } = await env.serverWallet.simulateContract({
				address,
				abi: EventExampleAbi,
				functionName: 'triggerEvent',
			})

			let hash = await env.serverWallet.writeContract(request)
			let receipt = await env.serverWallet.waitForTransactionReceipt({ hash })
			const logs = await env.serverWallet.getLogs({
				address,
				blockHash: receipt.blockHash,
			})
			expect(logs).toHaveLength(1)
			expect(logs[0]).toMatchObject({
				address,
				data: '0x00000000000000000000000000000000000000000000000000000000000030390000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000b48656c6c6f20776f726c64000000000000000000000000000000000000000000',
				transactionHash: hash,
			})

			expect(
				decodeEventLog({
					abi: EventExampleAbi,
					data: logs[0].data,
					topics: logs[0].topics,
				})
			).toEqual({
				eventName: 'ExampleEvent',
				args: {
					sender: env.serverWallet.account.address,
					value: 12345n,
					message: 'Hello world',
				},
			})
		})

		test('tracing', async () => {
			let [callerAddr, calleeAddr] = await getTracingExampleAddrs()

			const receipt = await (async () => {
				const { request } = await env.serverWallet.simulateContract({
					address: callerAddr,
					abi: TracingCallerAbi,
					functionName: 'start',
					args: [2n],
				})
				const hash = await env.serverWallet.writeContract(request)
				return await env.serverWallet.waitForTransactionReceipt({ hash })
			})()

			const visitor: Parameters<typeof visit>[1] = (key, value) => {
				switch (key) {
					case 'address':
					case 'from':
					case 'to': {
						if (value === callerAddr) {
							return '<contract_addr>'
						} else if (value === calleeAddr) {
							return '<contract_callee_addr>'
						} else if (value == env.serverWallet.account.address.toLowerCase()) {
							return '<caller>'
						}

						return value
					}
					case 'revertReason':
						return value.startsWith('revert: ') ? value.slice('revert: '.length) : value

					case 'gas':
					case 'gasUsed': {
						return '0x42'
					}
					case 'txHash': {
						return '<hash>'
					}
					default: {
						return value
					}
				}
			}

			// test debug_traceTransaction
			{
				const fixture = await Bun.file('./src/fixtures/trace_transaction.json').json()
				const res = await env.debugClient.traceTransaction(receipt.transactionHash, {
					withLog: true,
				})
				expect(visit(res, visitor)).toEqual(fixture)
			}

			// test debug_traceBlock
			{
				const res = await env.debugClient.traceBlock(receipt.blockNumber, { withLog: true })
				const fixture = await Bun.file('./src/fixtures/trace_block.json').json()
				expect(visit(res, visitor)).toEqual(fixture)
			}

			// test debug_traceCall
			{
				const fixture = await Bun.file('./src/fixtures/debug_traceCall.json').json()
				const res = await env.debugClient.traceCall(
					{
						to: callerAddr,
						data: encodeFunctionData({
							abi: TracingCallerAbi,
							functionName: 'start',
							args: [2n],
						}),
					},
					{ withLog: true }
				)
				expect(visit(res, visitor)).toEqual(fixture)
			}
		})
	})
}

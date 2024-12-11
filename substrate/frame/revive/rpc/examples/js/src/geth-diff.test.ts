import { jsonRpcErrors, procs, createEnv, getByteCode } from './geth-diff-setup.ts'
import { afterAll, afterEach, beforeAll, describe, expect, test } from 'bun:test'
import { encodeFunctionData, Hex, parseEther } from 'viem'
import { ErrorTesterAbi } from '../abi/ErrorTester'
import { FlipperCallerAbi } from '../abi/FlipperCaller'
import { FlipperAbi } from '../abi/Flipper'

afterEach(() => {
	jsonRpcErrors.length = 0
})

afterAll(async () => {
	procs.forEach((proc) => proc.kill())
})

const envs = await Promise.all([createEnv('geth'), createEnv('kitchensink')])

for (const env of envs) {
	describe(env.serverWallet.chain.name, () => {
		let errorTesterAddr: Hex = '0x'
		let flipperAddr: Hex = '0x'
		let flipperCallerAddr: Hex = '0x'
		beforeAll(async () => {
			{
				const hash = await env.serverWallet.deployContract({
					abi: ErrorTesterAbi,
					bytecode: getByteCode('errorTester', env.evm),
				})
				const deployReceipt = await env.serverWallet.waitForTransactionReceipt({ hash })
				if (!deployReceipt.contractAddress)
					throw new Error('Contract address should be set')
				errorTesterAddr = deployReceipt.contractAddress
			}

			{
				const hash = await env.serverWallet.deployContract({
					abi: FlipperAbi,
					bytecode: getByteCode('flipper', env.evm),
				})
				const deployReceipt = await env.serverWallet.waitForTransactionReceipt({ hash })
				if (!deployReceipt.contractAddress)
					throw new Error('Contract address should be set')
				flipperAddr = deployReceipt.contractAddress
			}

			{
				const hash = await env.serverWallet.deployContract({
					abi: FlipperCallerAbi,
					args: [flipperAddr],
					bytecode: getByteCode('flipperCaller', env.evm),
				})
				const deployReceipt = await env.serverWallet.waitForTransactionReceipt({ hash })
				if (!deployReceipt.contractAddress)
					throw new Error('Contract address should be set')
				flipperCallerAddr = deployReceipt.contractAddress
			}
		})

		test('triggerAssertError', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.readContract({
					address: errorTesterAddr,
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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

		test('eth_call transfer (not enough funds)', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.sendTransaction({
					to: '0x75E480dB528101a381Ce68544611C169Ad7EB342',
					value: parseEther('10'),
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
					abi: ErrorTesterAbi,
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

		test('eth_estimate call caller (not enough funds)', async () => {
			expect.assertions(3)
			try {
				await env.accountWallet.estimateContractGas({
					address: errorTesterAddr,
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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
					abi: ErrorTesterAbi,
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
				abi: ErrorTesterAbi,
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

		test.only('eth_estimate (no gas specified) child_call', async () => {
			let balance = await env.serverWallet.getBalance(env.accountWallet.account)
			expect(balance).toBe(0n)

			const data = encodeFunctionData({
				abi: FlipperCallerAbi,
				functionName: 'callFlip',
			})

			await env.accountWallet.request({
				method: 'eth_estimateGas',
				params: [
					{
						data,
						from: env.accountWallet.account.address,
						to: flipperCallerAddr,
						gas: `0x${Number(1000000).toString(16)}`,
					},
				],
			})
		})
	})
}

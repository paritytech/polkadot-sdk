//! Run with bun run script-revert.ts
import { call, getContract, deploy } from './lib.ts'

try {
	const { abi, bytecode } = getContract('revert')
	const address = await deploy(bytecode, abi)
	await call('doRevert', address, abi)
} catch (err) {
	console.error(err)
}

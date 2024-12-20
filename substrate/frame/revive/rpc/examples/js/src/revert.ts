//! Run with bun run script-revert.ts
import { call, getContract, deploy } from './lib.ts'

try {
	const { abi, bytecode } = getContract('revert')
	const contract = await deploy(bytecode, abi)
	await call('doRevert', await contract.getAddress(), abi)
} catch (err) {
	console.error(err)
}

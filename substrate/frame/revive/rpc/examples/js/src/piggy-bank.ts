//! Run with bun run script-event.ts
import { parseEther } from 'ethers'
import { call, getContract, deploy } from './lib.ts'

try {
	const { abi, bytecode } = getContract('piggyBank')
	const address = await deploy(bytecode, abi)

	let receipt = await call('deposit', address, abi, [], { value: parseEther('3.0') })
	console.log('Deposit Receipt:', receipt?.toJSON())

	receipt = await call('withdraw', address, abi, [parseEther('1.0')])
	console.log('Withdraw Receipt:', receipt?.toJSON())
} catch (err) {
	console.error(err)
}


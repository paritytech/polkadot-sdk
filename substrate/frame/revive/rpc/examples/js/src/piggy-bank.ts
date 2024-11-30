import { provider, call, getContract, deploy } from './lib.ts'
import { parseEther } from 'ethers'
import { PiggyBank } from '../types/ethers-contracts/PiggyBank'

try {
	const { abi, bytecode } = getContract('piggyBank')
	const contract = (await deploy(bytecode, abi)) as PiggyBank
	const address = await contract.getAddress()

	let receipt = await call('deposit', address, abi, [], {
		value: parseEther('10.0'),
	})
	console.log('Deposit receipt:', receipt?.status)
	console.log(`Contract balance: ${await provider.getBalance(address)}`)

	console.log('deposit: ', await contract.getDeposit())

	receipt = await call('withdraw', address, abi, [parseEther('5.0')])
	console.log('Withdraw receipt:', receipt?.status)
	console.log(`Contract balance: ${await provider.getBalance(address)}`)
	console.log('deposit: ', await contract.getDeposit())
} catch (err) {
	console.error(err)
}

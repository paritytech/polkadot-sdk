import { assert, getByteCode, walletClient } from './lib.ts'
import { abi } from '../abi/piggyBank.ts'
import { parseEther } from 'viem'

const hash = await walletClient.deployContract({
	abi,
	bytecode: getByteCode('piggyBank'),
})
const deployReceipt = await walletClient.waitForTransactionReceipt({ hash })
const contractAddress = deployReceipt.contractAddress
console.log('Contract deployed:', contractAddress)
assert(contractAddress, 'Contract address should be set')

// Deposit 10 WST
{
	const result = await walletClient.estimateContractGas({
		account: walletClient.account,
		address: contractAddress,
		abi,
		functionName: 'deposit',
		value: parseEther('10'),
	})

	console.log(`Gas estimate: ${result}`)

	const { request } = await walletClient.simulateContract({
		account: walletClient.account,
		address: contractAddress,
		abi,
		functionName: 'deposit',
		value: parseEther('10'),
	})

	request.nonce = 0
	const hash = await walletClient.writeContract(request)

	const receipt = await walletClient.waitForTransactionReceipt({ hash })
	console.log(`Deposit receipt: ${receipt.status}`)
	if (process.env.STOP) {
		process.exit(0)
	}
}

// Withdraw 5 WST
{
	const { request } = await walletClient.simulateContract({
		account: walletClient.account,
		address: contractAddress,
		abi,
		functionName: 'withdraw',
		args: [parseEther('5')],
	})

	const hash = await walletClient.writeContract(request)
	const receipt = await walletClient.waitForTransactionReceipt({ hash })
	console.log(`Withdraw receipt: ${receipt.status}`)

	// Check remaining balance
	const balance = await walletClient.readContract({
		address: contractAddress,
		abi,
		functionName: 'getDeposit',
	})

	console.log(`Get deposit: ${balance}`)
	console.log(
		`Get contract balance: ${await walletClient.getBalance({ address: contractAddress })}`
	)
}

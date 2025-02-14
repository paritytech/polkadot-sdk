import { assert, getByteCode, walletClient } from './lib.ts'
import { MyTokenAbi } from '../abi/myToken.ts'

const hash = await walletClient.deployContract({
	abi: MyTokenAbi,
	bytecode: getByteCode('MyToken'),
})
const deployReceipt = await walletClient.waitForTransactionReceipt({ hash })
const contractAddress = deployReceipt.contractAddress
console.log('Contract deployed:', contractAddress)
assert(contractAddress, 'Contract address should be set')

const getBalance = async (address: string) => {
	return await walletClient.readContract({
		address: contractAddress,
		abi: MyTokenAbi,
		functionName: 'balanceOf',
		args: [address],
	})
}

// Mint 1 MTK
{
	const oldBalance = await getBalance(walletClient.account.address)
	console.log(`Old Balance: ${oldBalance}`)

	const result = await walletClient.estimateContractGas({
		account: walletClient.account,
		address: contractAddress,
		abi: MyTokenAbi,
		functionName: 'mint',
		args: [walletClient.account.address, 1n],
	})
	console.log(`Gas estimate: ${result}`)

	const { request } = await walletClient.simulateContract({
		account: walletClient.account,
		address: contractAddress,
		abi: MyTokenAbi,
		functionName: 'mint',
		args: [walletClient.account.address, 1n],
	})

	const hash = await walletClient.writeContract(request)
	const receipt = await walletClient.waitForTransactionReceipt({ hash })
	console.log(`Mint receipt: ${receipt.status}`)

	const newBalance = await getBalance(walletClient.account.address)
	console.log(`New Balance: ${newBalance}`)
}

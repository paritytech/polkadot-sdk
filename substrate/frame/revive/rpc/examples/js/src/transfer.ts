import { parseEther } from 'ethers'
import { provider, signer } from './lib.ts'

const recipient = '0x75E480dB528101a381Ce68544611C169Ad7EB342'
try {
	console.log(`Signer balance:    ${await provider.getBalance(signer.address)}`)
	console.log(`Recipient balance: ${await provider.getBalance(recipient)}`)
	await signer.sendTransaction({
		to: recipient,
		value: parseEther('1.0'),
	})
	console.log(`Sent:              ${parseEther('1.0')}`)
	console.log(`Signer balance:    ${await provider.getBalance(signer.address)}`)
	console.log(`Recipient balance: ${await provider.getBalance(recipient)}`)
} catch (err) {
	console.error(err)
}

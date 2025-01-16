import { parseEther } from 'viem'
import { walletClient } from './lib.ts'

const recipient = '0x75E480dB528101a381Ce68544611C169Ad7EB342'
try {
	console.log(`Signer balance:    ${await walletClient.getBalance(walletClient.account)}`)
	console.log(`Recipient balance: ${await walletClient.getBalance({ address: recipient })}`)

	let resp = await walletClient.sendTransaction({
		to: recipient,
		value: parseEther('1.0'),
	})
	console.log(`Transaction hash:  ${resp}`)
	console.log(`Sent:              ${parseEther('1.0')}`)
	console.log(`Signer balance:    ${await walletClient.getBalance(walletClient.account)}`)
	console.log(`Recipient balance: ${await walletClient.getBalance({ address: recipient })}`)
} catch (err) {
	console.error(err)
}

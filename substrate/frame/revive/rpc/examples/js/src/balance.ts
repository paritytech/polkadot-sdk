import { walletClient } from './lib.ts'

const recipient = '0x8D97689C9818892B700e27F316cc3E41e17fBeb9'
try {
	console.log(`Recipient balance: ${await walletClient.getBalance({ address: recipient })}`)
} catch (err) {
	console.error(err)
}

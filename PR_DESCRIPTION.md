# pallet-asset-conversion: keep the sender account alive during a swap

A swap now holds a sufficient reference on the sender's account while it runs, so that the account stays alive for the whole call. If the sender ends the swap with no other reference, the account is reaped when the reference is released, as before.

The amounts, the events and the conditions under which a swap succeeds or fails are unchanged.

## Alternative considered

Credit the receiver before debiting the sender. It needs no reference, but for a swap it changes behaviour:

1. A circular swap (for example Native → A → B → Native) no longer needs the input amount up front, because the sender can pay the input out of the output.
2. A sender at the consumer limit that swaps all of one non-sufficient asset for a new non-sufficient one fails, because it holds both for a moment.
3. The asset events of the output leg come before those of the input leg.
4. When both a pool-side and a sender-side problem apply, the pool-side error is returned first.

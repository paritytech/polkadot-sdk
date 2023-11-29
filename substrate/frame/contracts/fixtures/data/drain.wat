(module
	(import "seal0" "seal_balance" (func $seal_balance (param i32 i32)))
	(import "seal0" "seal_minimum_balance" (func $seal_minimum_balance (param i32 i32)))
	(import "seal0" "seal_transfer" (func $seal_transfer (param i32 i32 i32 i32) (result i32)))
	(import "env" "memory" (memory 1 1))

	;; [0, 8) reserved for $seal_balance output

	;; [8, 16) length of the buffer for $seal_balance
	(data (i32.const 8) "\08")

	;; [16, 24) reserved for $seal_minimum_balance

	;; [24, 32) length of the buffer for $seal_minimum_balance
	(data (i32.const 24) "\08")

	;; [32, inf) zero initialized

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(local.get 0)
			)
			(unreachable)
		)
	)

	(func (export "deploy"))

	(func (export "call")
		;; Send entire remaining balance to the 0 address.
		(call $seal_balance (i32.const 0) (i32.const 8))

		;; Balance should be encoded as a u64.
		(call $assert
			(i32.eq
				(i32.load (i32.const 8))
				(i32.const 8)
			)
		)

		;; Get the minimum balance.
		(call $seal_minimum_balance (i32.const 16) (i32.const 24))

		;; Minimum balance should be encoded as a u64.
		(call $assert
			(i32.eq
				(i32.load (i32.const 24))
				(i32.const 8)
			)
		)

		;; Make the transferred value exceed the balance by adding the minimum balance.
		(i64.store (i32.const 0)
			(i64.add
				(i64.load (i32.const 0))
				(i64.load (i32.const 16))
			)
		)

		;; Try to self-destruct by sending more balance to the 0 address.
		;; The call will fail because a contract transfer has a keep alive requirement
		(call $assert
			(i32.eq
				(call $seal_transfer
					(i32.const 32)	;; Pointer to destination address
					(i32.const 48)	;; Length of destination address
					(i32.const 0)	;; Pointer to the buffer with value to transfer
					(i32.const 8)	;; Length of the buffer with value to transfer
				)
				(i32.const 5) ;; ReturnCode::TransferFailed
			)
		)
	)
)

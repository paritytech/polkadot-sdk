(module
	(import "seal0" "seal_balance" (func $seal_balance (param i32 i32)))
	(import "env" "memory" (memory 1 1))

	;; [0, 8) reserved for $seal_balance output

	;; [8, 16) length of the buffer for $seal_balance
	(data (i32.const 8) "\08")

	;; [16, inf) zero initialized

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
		(call $seal_balance (i32.const 0) (i32.const 8))

		;; Balance should be encoded as a u64.
		(call $assert
			(i32.eq
				(i32.load (i32.const 8))
				(i32.const 8)
			)
		)

		;; Assert the free balance to be zero.
		(call $assert
			(i64.eq
				(i64.load (i32.const 0))
				(i64.const 0)
			)
		)
	)
)

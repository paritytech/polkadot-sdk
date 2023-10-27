;; This passes its input to `seal_xcm_query` and returns the return value to its caller.
(module
	(import "seal0" "xcm_query" (func $xcm_query (param i32 i32 i32) (result i32)))
	(import "seal0" "seal_input" (func $seal_input (param i32 i32)))
	(import "seal0" "seal_return" (func $seal_return (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	;; 0x1000 = 4k in little endian
	;; size of input buffer
	(data (i32.const 0) "\00\10")

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(get_local 0)
			)
			(unreachable)
		)
	)

	(func (export "call")
		;; Receive the encoded call
		(call $seal_input
			(i32.const 4)	;; Pointer to the input buffer
			(i32.const 0)	;; Pointer to the buffer length (before call) and to the copied data length (after call)
		)
		;; Input data layout.
		;; [0..4) - size of the input buffer
		;; [4..12) - timeout
		;; [12..49) - match_querier

		;; Call xcm_query with provided input.
		(call $assert
			(i32.eq
				(call $xcm_query
					(i32.const 4)   ;; Pointer where the timeout is stored
					(i32.const 12)	;; Pointer where the match_querier is stored
					(i32.const 49)	;; Pointer to the where the query_id is stored
				)
				(i32.const 0)
			)
		)

		;; Return the the query_id
		(call $seal_return
			(i32.const 0)	;; flags
			(i32.const 49)	;; Pointer to returned value
			(i32.const 8)	;; length of returned value
		)
	)

	(func (export "deploy"))
)

;; This passes its input to `seal_xcm_send` and returns the return value to its caller.
(module
	(import "seal0" "xcm_send" (func $xcm_send (param i32 i32 i32 i32) (result i32)))
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
			(i32.const 0)	;; Size of the length buffer
		)

		;; Input data layout.
		;; [0..4) - size of the call
		;; [4..7) - dest
		;; [7..) - message

		;; Call xcm_send with provided input.
		(call $assert
			(i32.eq
				(call $xcm_send
					(i32.const 4)               ;; Pointer where the dest is stored
					(i32.const 7)				;; Pointer where the message is stored
					(i32.sub
						(i32.load (i32.const 0)) ;; length of the input buffer
						(i32.const 3)            ;; Size of the XCM dest
					)
					(i32.const 100)	            ;; Pointer to the where the message_id is stored
				)
				(i32.const 0)
			)
		)

		;; Return the the message_id
		(call $seal_return
			(i32.const 0)	;; flags
			(i32.const 100)	;; Pointer to returned value
			(i32.const 32)	;; length of returned value
		)
	)

	(func (export "deploy"))
)

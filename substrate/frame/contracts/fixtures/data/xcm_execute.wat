;; This passes its input to `seal_xcm_execute` and returns the return value to its caller.
(module
	(import "seal0" "xcm_execute" (func $xcm_execute (param i32 i32 i32) (result i32)))
	(import "seal0" "seal_input" (func $seal_input (param i32 i32)))
	(import "seal0" "seal_return" (func $seal_return (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	;; 0x1000 = 4k in little endian
	;; Size of input buffer
	(data (i32.const 0) "\00\10")

	(func $assert (param i32)
		(block $ok
			(br_if $ok
				(local.get 0)
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
		;; [0..4) - size of the call
		;; [4..) - message

		;; Call xcm_execute with provided input.
		(call $assert
			(i32.eq
				(call $xcm_execute
					(i32.const 4)		     ;; Pointer where the message is stored
					(i32.load (i32.const 0)) ;; Size of the message
					(i32.const 100)	         ;; Pointer to the where the outcome is stored
				)
				(i32.const 0)
			)
		)

		(call $seal_return
			(i32.const 0)	;; flags
			(i32.const 100)	;; Pointer to returned value
			(i32.const 10)	;; length of returned value
		)
	)

	(func (export "deploy"))
)


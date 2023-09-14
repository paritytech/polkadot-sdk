;; This passes its input to `seal_xcm_execute` and returns the return value to its caller.
(module
	(import "seal0" "xcm_execute" (func $xcm_execute (param i32 i32) (result i32)))
	(import "seal0" "seal_input" (func $seal_input (param i32 i32)))
	(import "seal0" "seal_return" (func $seal_return (param i32 i32 i32)))
	(import "env" "memory" (memory 1 1))

	;; 0x1000 = 4k in little endian
	;; Size of input buffer
	(data (i32.const 0) "\00\10")

	(func (export "call")
		;; Receive the encoded call
		(call $seal_input
			(i32.const 4)	;; Pointer to the input buffer
			(i32.const 0)	;; Size of the length buffer
		)
		;; Input data layout.
		;; [0..4) - size of the call
		;; [4..) - xcm message

		;; Just use the call passed as input and store result to memory
		(i32.store (i32.const 0)
			(call $xcm_execute
				(i32.const 4)		     ;; Pointer where the xcm msg is stored
				(i32.load (i32.const 0)) ;; Size of the xcm msg
			)
		)
		(call $seal_return
			(i32.const 0)	;; flags
			(i32.const 0)	;; returned value
			(i32.const 4)	;; length of returned value
		)
	)

	(func (export "deploy"))
)


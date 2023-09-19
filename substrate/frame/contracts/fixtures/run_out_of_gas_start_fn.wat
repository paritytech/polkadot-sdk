(module
	(import "env" "memory" (memory 1 1))
	(start $start)
	(func $start
		(loop $inf (br $inf)) ;; just run out of gas
		(unreachable)
	)
	(func (export "call"))
	(func (export "deploy"))
)

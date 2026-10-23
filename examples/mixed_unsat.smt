# CDCL(T) forces both incompatible difference constraints
var x 0 4
var y 0 4
atom a: x-y <= -1
atom b: y-x <= -1
atom c: x+y >= 4
clause a b
clause !a b
clause a !b
clause c

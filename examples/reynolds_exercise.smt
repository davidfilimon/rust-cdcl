# VTSA 2017, Andrew Reynolds, slide 54+, exercitiu DPLL(LIA)
# (x>y OR x>z) AND (x+1<y OR NOT(x>y)) AND (x>y OR z>y)
var x -8 8
var y -8 8
var z -8 8
atom a: x > y
atom b: x > z
atom c: x+1 < y
atom d: z > y
clause a b
clause c !a
clause a d

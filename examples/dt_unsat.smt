# Difference theory: UNSAT via negative cycle
var x -10 10
var y -10 10
atom a: x-y <= -1
atom b: y-x <= -1
clause a
clause b

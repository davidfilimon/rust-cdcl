# VTSA 2017, Andrew Reynolds, DPLL(T) - exemplele de la slide-urile 35-52
# F = (x+1>0 OR x+y>0) AND (x<0 OR x+y>4) AND NOT(x+y>0)
# Rezultat: UNSAT; o lema valida este (x+y>0 OR NOT(x+y>4)).
var x -10 10
var y -10 10
atom a: x+1 > 0
atom b: x+y > 0
atom c: x < 0
atom d: x+y > 4
clause a b
clause c d
clause !b

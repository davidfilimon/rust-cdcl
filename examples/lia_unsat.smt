# Linear integer arithmetic: UNSAT
var x 0 10
atom low: 2*x <= 3
atom high: x >= 3
clause low
clause high

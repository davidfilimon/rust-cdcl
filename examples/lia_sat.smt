# Linear integer arithmetic: SAT
var x 0 8
var y 0 8
atom budget: 2*x+3*y <= 16
atom min: x+y >= 5
clause budget
clause min

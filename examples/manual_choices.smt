# Model de exemplu pentru alegeri: b=true -> c=true este conflict
var x 0 5
atom a: x >= 2
atom b: x <= 3
atom c: x >= 4
clause a b

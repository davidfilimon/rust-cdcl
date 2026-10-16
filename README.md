# CDCL SAT Solver in Rust

Solver SAT educational, fara dependinte externe, care accepta fisiere **DIMACS CNF** si implementeaza CDCL (Conflict-Driven Clause Learning).

> Nota: `Tmax` nu este o extensie de fisier recunoscuta ca standard SAT. In acest proiect reprezinta **timpul maxim de executie in secunde**, configurat prin `--tmax`. Fisierele de intrare sunt `.cnf`. Daca profesorul foloseste alt sens pentru Tmax, trebuie ajustata doar interfata.

## Ce este implementat

| Functionalitate | Status | Explicatie |
|---|---|---|
| Parser DIMACS CNF | DA | mai multe clauze pe linie, clauze pe linii multiple, comentarii, verificare header |
| Evaluarea formulelor / modelului | DA | `--check-model` si functia `verify_model` |
| Simplificare clauze | DA | eliminarea duplicatelor si a tautologiilor la incarcare |
| BCP / unit propagation | DA | **two-watched literals (2WL)** |
| Alegerea variabilei | DA | **VSIDS** sau prima variabila libera |
| Phase saving | DA | reutilizeaza polaritatea ultimei atribuiri |
| Decision levels / implication graph | DA | nivel + clauza cauza pentru fiecare atribuire |
| Conflict detection | DA | clauza nesatisfacuta |
| Conflict analysis | DA | **First-UIP**, rezolutie pe motive |
| Clause learning | DA | memorarea clauzelor invatate |
| Non-chronological backjump | DA | sare la cel mai mare nivel relevant |
| Restarts | DA | secventa **Luby**, setabila si dezactivabila |
| Scorul clauzelor / LBD | DA | numar de niveluri distincte la invatare |
| Reducerea bazei de clauze | DA | sterge periodic clauze lungi, neblocate, mai putin utile |
| Limita de timp (`Tmax`) | DA | returneaza `UNKNOWN` la expirarea timpului |
| Limita de conflicte | DA | `--max-conflicts` |
| Log de executie | DA | `--trace` afiseaza decizii, conflicte, clauze, backjump si restarts |
| Statistici | DA | decizii, conflicte, propagari, clauze, restarturi, timp |
| Generator de probleme | DA | Random 3-SAT, Pigeonhole, N-Queens |
| Benchmark si CSV | DA | ruleaza toate fisierele `.cnf` din director |
| Verificare cu brute-force | DA | teste random mici pentru a compara SAT/UNSAT |

**Nu este un solver industrial**: nu implementeaza inprocessing sofisticat, incremental SAT/assumptions, DRAT/LRAT, core extraction sau multe optimizari MiniSat/CaDiCaL. Fara aceste tehnici, instantele mari pot fi lente.

## Instalare

Instaleaza Rust de la https://rustup.rs/ (macOS: `brew install rust` este o alternativa), apoi:

```bash
cd cdcl-rust-plus
cargo test
cargo build --release
```

Ruleaza cu `cargo run` in development sau `cargo run --release` pentru viteza. Atentie: la SAT Competition, codurile de iesire standard sunt **10 (SAT)** si **20 (UNSAT)**; prin urmare, `cargo run` poate semnala `process exited with status 10/20` chiar daca raspunsul este corect. Pentru scripturi, foloseste binarul `./target/release/cdcl-rust`.

## Rulare

```bash
cargo run --release -- solve benchmarks/sat_simple.cnf --check-model
cargo run --release -- solve benchmarks/unsat_xor.cnf --trace
cargo run --release -- solve benchmarks/pigeonhole_4.cnf --tmax 5
cargo run --release -- solve benchmarks/sat_simple.cnf --max-conflicts 1000
cargo run --release -- solve benchmarks/pigeonhole_4.cnf --heuristic first --no-restarts
```

Format iesire (conventie DIMACS):

```text
c decisions        : 3
c conflicts        : 1
... statistici ...
s SATISFIABLE
v 1 -2 3 0
```

`SATISFIABLE` = exista o atribuire satisfacatoare; `UNSATISFIABLE` = nicio atribuire; `UNKNOWN` = nu a terminat la timp / limita conflicte. O linie de model `v` listeaza variabilele pozitive/negative urmate de 0. Valorile exacte pot sa difere.

## Generare teste

```bash
cargo run --release -- generate random3 --n 30 --m 128 --seed 42 --output benchmarks/random_30.cnf
cargo run --release -- generate pigeonhole --n 4 --output benchmarks/php4.cnf
cargo run --release -- generate queens --n 8 --output benchmarks/queens8.cnf
```

Interpretare: Random 3-SAT = clauze cu 3 literali aleatori; Pigeonhole n = n+1 porumbei in n cuiburi (UNSAT); N-Queens n = asezarea a n regine fara atac reciproc (n=1/4/8 SAT, n=2/3 UNSAT).

## Benchmark Tmax

```bash
cargo run --release -- bench benchmarks --tmax 10 --csv results.csv
```

CSV contine: `file, vars, clauses, status, seconds, decisions, propagations, conflicts, learned, deleted, restarts, backjumps`.

Poti compara modul cu VSIDS versus prima variabila libera:

```bash
cargo run --release -- bench benchmarks --tmax 10 --csv vsids.csv
cargo run --release -- bench benchmarks --tmax 10 --heuristic first --csv first.csv
```

Nu compara riguros timpii compilati cu `cargo run` in debug: pentru benchmark foloseste `--release`. Timpul raportat exclude parsarea si compilarea.

## Structura

```text
Cargo.toml
src/lib.rs             # algoritmul CDCL + parser + generator + teste
src/main.rs            # CLI: solve / bench / generate
benchmarks/*.cnf       # instante de test
README.md              # aceasta documentatie
ALGORITM.md            # CDCL explicat si pseudocod
```

## Explicatie scurta

1. Parseaza clauzele si aplica asigurarile la nivel zero (unit clauses).
2. Propaga implicatiile cu two-watched literals.
3. Daca apare un conflict la nivel zero, problema este UNSAT.
4. Altfel, analizeaza graful implicatiilor prin First-UIP si invata o clauza.
5. Face backjump la nivelul calculat, afirma literalul din clauza nou invatata.
6. Periodic: VSIDS decay, restarts cu Luby si reducerea bazei invatate.
7. Daca nu exista conflict, alege o variabila libera. Daca toate variabilele au valoare, returneaza SAT.

## Limitari de implementare

- Limita de timp este verificata intre apelurile de propagare, nu prin intrerupere preemptiva in mijlocul unui apel lung.
- Ordinea deciziilor este determinista pentru aceeasi intrare.
- Eliminarea clauzelor este simplificata: pastreaza clause IDs stable, sterge logic si curata watchers la propagare.
- DIMACS trebuie sa aiba header `p cnf N M` si clauzele terminate cu zero.
- Niciun argument extern nu este instalat automat.

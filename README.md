# CDCL(T) Visual Solver — slide-style debugger

Educational SAT/SMT solver written in Rust, with step-by-step replay of **real events from the Rust solver**, not a JavaScript reimplementation.

This edition's UI is inspired by Andrew Reynolds, *DPLL(T) for SMT*, VTSA 2017, especially slides 35–67:
https://conferences.mpi-inf.mpg.de/rg1/vtsa17/slides/reynolds-vtsa-part1.pdf

## Run on macOS

```bash
cargo test
cargo run --release --bin smt -- web
```

Open http://127.0.0.1:7878 and select **VTSA · Slide 50** or **VTSA · Exercițiu 54–67**. Press **Pregătește execuția** and use **Next →** to move through each actual event.

- **Formula:** the original clauses and subsequently learned clauses, with per-literal evaluation (green/true, orange/false, gray/unassigned).
- **Context:** truth assignments and decision levels, updated on each replay step.
- **Learned clauses:** CDCL First-UIP clauses and SMT theory lemmas, separated by category.
- **Why a lemma is sound:** a theory-UNSAT core `l1 ∧ ... ∧ ln` entails the clause `¬l1 ∨ ... ∨ ¬ln`. The SAT solver re-runs after the SMT lemma is added.
- **Adaugă clauzele în editor:** inserts learned clauses into the original `.smt`-like format or DIMACS `.cnf` (updates the header clause count in CNF), and resets replay. Run again to see the effects.
- **Export:** copy learned clauses in the corresponding syntax.
- **Controls:** Next / Previous, Home / End, Autoplay / Pause, slider, left/right arrow keys, fullscreen presentation mode.

Run the included test exercise using:

```bash
cargo run --release --bin smt -- examples/reynolds_slide50.smt --trace
cargo run --release --bin smt -- examples/reynolds_exercise.smt --trace
```

## Important scope

This is an educational, **bounded integer LIA** and difference-logic SMT implementation. A line such as `var x -10 10` limits `x` to that integer range and affects the logical result. It's not a full QF_LIA solver over unbounded integers. `UNKNOWN` may occur on timeouts. The Rust engine calculates each result first; the UI replays the saved execution — `Next` does **not** advance a live paused Rust thread.

The original optimized SAT solver is still available:

```bash
cargo run --release --bin cdcl-rust -- solve benchmarks/unsat_xor.cnf
```

## Verify the interface without starting Rust

```bash
node test-ui.cjs
```

The test suite also includes Rust tests that check theory-lemma soundness by enumerating every in-range integer assignment for the two VTSA examples.

## NEW: User-guided DPLL(T) decisions

See [README-MANUAL.md](README-MANUAL.md). The visual debugger now supports choosing `a / b / c / d = TRUE / FALSE` with green/red feasibility checks performed by Rust and proof-checked LIA/DT theory lemmas.

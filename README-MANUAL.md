# DPLL(T) interactiv: deciziile tale, nu decizii inventate

Aceasta versiune **pastreaza CDCL si CDCL(T) automatele existente** si adauga un mod separat de explorare manuala pentru formule SMT cu `atom` si `clause`.

## Pornire

```bash
cargo test
cargo run --release --bin smt -- web
```

Deschide `http://127.0.0.1:7878`.

1. Alege **VTSA · Slide 50 (UNSAT)** si apasa **Eu aleg deciziile (verde / rosu)**.
2. Rust face mai intai *Boolean Unit Propagation*: `b = false`, apoi `a = true`.
3. Butoanele pentru `c` si `d` ofera optiuni de decizie TRUE / FALSE. **Verde** inseamna ca alegerea si toate propagările booleene rezultate sunt compatibile cu teoria pe domeniile declarate. **Rosu** inseamna conflict demonstrat (boolean sau aritmetic). Verde NU inseamna ca formula initiala este global SAT.
4. Daca alegi `c = true`, apare conflictul de teorie `{a,c}`, care permite lema valida `!a OR !c`.
5. Apasa **Invata lema de conflict**. Solverul Rust reverifica lema ca tautologie in teoria aritmeticii intregi, revine la nivelul 0 si propaga clauza noua.
6. Daca alegi `d = true` din contextul original, conflictul `{!b,d}` permite lema `b OR !d`.
7. După prima lemă, propagarea poate descoperi automat al doilea conflict de teorie `{!b,d}`; îl poți transforma în lema `b OR !d`. Cu ambele leme, propagarea booleană identifică UNSAT la nivelul 0.
8. Cu **Next/Previous** poti naviga prin fiecare pas inregistrat. Deciziile noi se iau mereu raportat la cel mai recent context, nu la un pas istoric selectat pentru vizualizare.

Aplicatia afiseaza exclusiv numele atomilor declarati de utilizator (`a`, `b`, `c`, `d`), inclusiv in clauzele invatate. Formula afiseaza expresiile matematice si identificatorii atomilor.

## Pentru o ramura verde, apoi una rosie

Selecteaza **Decizii · Verde / Rosu** sau deschide `examples/manual_choices.smt`:

```text
var x 0 5
atom a: x >= 2
atom b: x <= 3
atom c: x >= 4
clause a b
```

- Click `b=TRUE`: compatibil (verde).
- Apoi click `c=TRUE`: conflict (rosu), deoarece `x<=3` si `x>=4` nu au solutie comuna.
- Lema invatata este `!b OR !c`.

## Arhitectura

- `src/smt.rs`: `manual_check` - propagare CNF, verificare exacta a constrangerilor LIA finite / DT, extragere de nucleu conflictual prin eliminare si validare lema.
- `src/bin/smt.rs`: endpoint local `POST /api/manual`, verificare suplimentara a lemelor trimise de browser; endpoint-ul precedent `POST /api/solve` ramane.
- `web/index.html`: butoane pentru literalele declarate, evaluare verde / rosu, invatare lema, istoric si replay Next/Previous.
- `test-manual-ui.cjs`: testeaza interactiunile in JavaScript fara a simula validitatea aritmetica.
- Testele unitare Rust verifica exemplul slide 50 si un scenariu SAT cu decizii verzi si rosii.

## Limite / diferente importante

Acesta este un **explorator didactic DPLL(T)**, nu un CDCL(T) incremental industrial. Alegerea utilizatorului este evaluata pe loc, insa nucleele de conflict sunt obtinute din teoria numerelor intregi cu domeniile `var` declarate. Dupa invatarea unei leme, modul manual revine intentionat la nivelul 0. Modul **Pregateste executia automata** continua sa ruleze CDCL(T) automat, cu trace si analiza de conflict booleana First-UIP.

`UNSAT` poate fi declarat doar pentru un conflict la nivelul 0. `SAT` in modul manual apare dupa atribuirea tuturor atomilor si gasirea unui model intreg. Alegerea verde atesta doar fezabilitatea contextului curent.

## Testare

```bash
cargo test
node test-ui.cjs
node test-manual-ui.cjs
```

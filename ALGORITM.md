# Algoritmul CDCL - explicatie pentru prezentare

SAT = decidem daca o formula booleana in forma normala conjunctiva (CNF) poate fi facuta adevarata.

CNF este o conjunctie de clauze: `(x1 OR x2) AND (!x1 OR x3)`.
Fiecare clauza este o disjunctie de literali. `1 -2 0` inseamna `(x1 OR NOT x2)`.

## Pseudocod

```text
CDCL(F):
  simplifica clauzele si initializeaza atribuirile unitare
  cat timp e adevarat:
    conflict = BCP_cu_doi_literali_urmariti()
    daca conflict:
      daca nivel_decizie == 0: return UNSAT
      C = analiza_conflictului_First_UIP(conflict)
      nivel = al_doilea_cel_mai_mare_nivel_din_C()
      invata(C)
      backjump(nivel)
      propaga_literalul_asertiv_din_C()
      optional_restart_si_reducere()
    altfel:
      daca toate variabilele sunt atribuite: return SAT si modelul
      alege_variabila_dupa_VSIDS()
      incepe_nivel_nou_de_decizie()
```

## 1. Two-watched literals

O clauza cu mai multi literali pastreaza doi literali observati. Cand unul devine fals, incercam sa mutam observatorul pe un alt literal ne-fals. Daca nu se poate, cealalta observatie poate fi fortata (unit propagation) sau conflict. Scade numarul de clauze vizitate pentru fiecare asignare.

## 2. Decision level, reason, trail

- `trail`: ordinea atribuirilor de literali.
- `trail_limits`: indexul de inceput al fiecarui nivel de decizie.
- `levels[v]`: nivelul la care a fost atribuita variabila.
- `reasons[v]`: id-ul clauzei care a fortat atribuirea, daca exista.

Impreuna, acestea codifica graful de implicatii fara un graf explicit.

## 3. First-UIP

La conflict, rezolvam repetat clauza conflictului cu clauza-motiv a celei mai recente atribuiri marcate de la nivelul curent. Ne oprim cand ramane un singur literal din nivelul curent. Acesta este literalul asertiv (`learnt[0]`). Clauza invatata previne repetarea aceleiasi combinatii incompatibile.

## 4. Non-chronological backjump

Din ceilalti literali ai clauzei invatate, alegem nivelul de decizie maxim. Revenim **direct** la acel nivel si propagam literalul asertiv. Daca nu exista ceilalti literali, revenim la nivel zero.

## 5. VSIDS si phase saving

Dupa un conflict, crestem activitatea variabilelor importante; periodic normalizam/decay scorul. Urmatoarea decizie este variabila cu activitatea maxima, alegand ultima polaritate incercata pentru ea.

## 6. Restarts Luby

Dupa un anumit numar de conflicte, solverul revine la nivel zero fara a sterge clauzele invatate. Secventa intervalelor este `1,1,2,1,1,2,4,...` inmultita cu `restart_base`.

## 7. LBD si reducere

LBD = numarul de niveluri distincte prezente in clauza invatata. Clauzele cu LBD mic tind sa fie valoroase. Clauzele invatate lungi/neblocate, cu LBD mare si activitate redusa, pot fi marcate ca eliminate. Nu se elimina o clauza folosita ca motiv al unei atribuiri active.

## 8. TMAX

`--tmax 5` impune cel mult ~5 secunde de calcul (timp de ceas, cu verificare periodica). Daca nu exista concluzie, raspunsul este `UNKNOWN`, **nu** `UNSAT`. Testele `.cnf` nu au o extensie speciala `.tmax`.

## Posibile extensii avansate

- DRAT/LRAT proof logging si verificarea certificatelor UNSAT
- Assumptions si incremental solving prin interfata de biblioteca
- Preprocesare: pure literal elimination, subsumption, variable elimination
- Heuristica EVSIDS cu priority heap (acum selectia scaneaza variabilele)
- 2WL cu watch-list garbage collection mai agresiv
- Conflict clause minimization, vivification si inprocessing
- Benchmark pe seturi industriale si comparatie cu MiniSat/CaDiCaL

//! Bounded LIA + integer difference logic (DT) via a CDCL(T) lazy loop.
//! No external dependencies.  Bounds belong to the input semantics.
use crate::{Cnf, Options, Solver, SolverStep, Status, MAX_DEBUG_STEPS};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Atom { pub name: String, pub coeffs: Vec<i128>, pub bound: i128, pub original: String }
#[derive(Clone, Debug)]
pub struct Problem {
    pub vars: Vec<(String, i64, i64)>,
    pub atoms: Vec<Atom>,
    pub clauses: Vec<Vec<i32>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict { Sat, Unsat, Unknown }
#[derive(Clone, Debug)]
pub struct Answer {
    pub verdict: Verdict,
    pub integers: Vec<(String, i64)>,
    pub booleans: Vec<(String, bool)>,
    pub iterations: usize,
    pub theory_conflicts: usize,
    pub decisions: u64,
    pub conflicts: u64,
    pub trace: Vec<String>,
    pub events: Vec<StepEvent>,
    pub events_truncated: bool,
    pub elapsed: Duration,
}
/// Structured, replayable events captured during the actual SAT/SMT run.
#[derive(Clone, Debug)]
pub struct StepEvent {
    pub kind: &'static str,
    pub source: &'static str,
    pub message: String,
    pub iteration: usize,
    pub level: usize,
    pub trail: Vec<i32>,
    pub levels: Vec<usize>,
    pub clause: Vec<i32>,
    pub decisions: u64,
    pub conflicts: u64,
}

fn push_event(events:&mut Vec<StepEvent>, truncated:&mut bool, enabled:bool, event:StepEvent) {
    if !enabled {return;}
    if events.len() >= MAX_DEBUG_STEPS { *truncated=true; return; }
    events.push(event);
}
fn info_event(events:&mut Vec<StepEvent>,truncated:&mut bool, enabled:bool,
              kind:&'static str,source:&'static str,message:String,iteration:usize,
              decisions:u64,conflicts:u64,clause:Vec<i32>) {
    if !enabled {return;}
    let (level,trail,levels)=if kind=="iteration" || kind=="start" { (0,vec![],vec![]) }
        else { events.last().map(|e|(e.level,e.trail.clone(),e.levels.clone())).unwrap_or_default() };
    push_event(events,truncated,true,StepEvent{kind,source,message,iteration,level,trail,levels,clause,decisions,conflicts});
}
fn add_cdcl_events(events:&mut Vec<StepEvent>,truncated:&mut bool,enabled:bool,
                   iteration:usize,total_decisions:u64,total_conflicts:u64,solver:&Solver){
    if !enabled {return;}
    if solver.steps_truncated {*truncated=true;}
    for SolverStep {kind,message,level,trail,levels,clause,decisions,conflicts} in &solver.steps {
        // A Boolean SAT model is only a candidate until the theory check passes.
        let kind=if *kind=="sat" {"boolean_sat"} else {*kind};
        push_event(events,truncated,true,StepEvent{kind,source:"cdcl",message:message.clone(),iteration,
            level:*level,trail:trail.clone(),levels:levels.clone(),clause:clause.clone(),
            decisions:total_decisions+*decisions,conflicts:total_conflicts+*conflicts});
    }
}

fn parse_i64(s: &str) -> Result<i64, String> { s.parse::<i64>().map_err(|_| format!("invalid integer: {s}")) }
fn linear_expr(expr: &str) -> Result<(BTreeMap<String, i128>, i128), String> {
    let s: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() { return Err("empty arithmetic expression".into()); }
    let b = s.as_bytes(); let mut i = 0; let mut co = BTreeMap::<String,i128>::new(); let mut constant=0_i128;
    while i < b.len() {
        let mut sign=1_i128;
        if b[i] == b'+' { i+=1; } else if b[i] == b'-' { sign=-1; i+=1; }
        if i == b.len() { return Err(format!("trailing sign in {expr}")); }
        let begin=i;
        while i<b.len() && b[i].is_ascii_digit() {i+=1;}
        let number=if i>begin {Some(s[begin..i].parse::<i128>().map_err(|_| "integer overflow")?)} else {None};
        if i<b.len() && b[i]==b'*' { if number.is_none() { return Err(format!("expected number before * in {expr}")); } i+=1; }
        if i<b.len() && (b[i].is_ascii_alphabetic() || b[i]==b'_') {
            let begin=i; i+=1;
            while i<b.len() && (b[i].is_ascii_alphanumeric() || b[i]==b'_') {i+=1;}
            let name=&s[begin..i];
            let value=sign.checked_mul(number.unwrap_or(1)).ok_or("integer overflow")?;
            *co.entry(name.into()).or_default() += value;
        } else {
            let value=number.ok_or_else(||format!("bad term in {expr} at {i}"))?;
            constant += sign*value;
        }
        if i<b.len() && b[i]!=b'+' && b[i]!=b'-' { return Err(format!("unexpected character in {expr}: {}",b[i] as char)); }
    }
    Ok((co,constant))
}
fn parse_relation(expr: &str, vars: &[(String,i64,i64)]) -> Result<(Vec<i128>,i128),String> {
    let operators=["<=",">=","<",">"];
    let (op,at)=operators.iter().filter_map(|op|expr.find(op).map(|i|(*op,i))).min_by_key(|x|x.1).ok_or("atom needs <=, >=, <, or >")?;
    let (lhs,lc)=linear_expr(&expr[..at])?;
    let (rhs,rc)=linear_expr(&expr[at+op.len()..])?;
    let mut map=lhs;
    for (k,v) in rhs { *map.entry(k).or_default()-=v; }
    let mut bound=rc-lc;
    if op=="<" {bound-=1;}
    if op==">" {bound+=1;}
    if op==">=" || op==">" {bound=-bound; for v in map.values_mut(){*v=-*v;} }
    let mut result=vec![0;vars.len()];
    for (k,v) in map { let ix=vars.iter().position(|x|x.0==k).ok_or_else(||format!("undeclared integer variable {k}"))?; result[ix]=v; }
    Ok((result,bound))
}
pub fn parse_problem(source: &str) -> Result<Problem,String> {
    let mut vars:Vec<(String,i64,i64)>=vec![];
    let mut declarations:Vec<(String,String)>=vec![];
    let mut raw_clauses:Vec<(usize,String)>=vec![];
    let mut names=std::collections::HashSet::new();
    for (index,line) in source.lines().enumerate(){
        let line=line.split('#').next().unwrap_or("").trim(); if line.is_empty(){continue;}
        if let Some(rest)=line.strip_prefix("var ") {
            let parts:Vec<_>=rest.split_whitespace().collect();
            if parts.len()!=3 || !is_name(parts[0]) {return Err(format!("line {}: syntax: var x MIN MAX",index+1));}
            let lo=parse_i64(parts[1])?; let hi=parse_i64(parts[2])?;
            if lo>hi {return Err(format!("line {}: min > max",index+1));}
            if !names.insert(parts[0].to_string()){return Err(format!("line {}: duplicate variable",index+1));}
            vars.push((parts[0].into(),lo,hi));
        }else if let Some(rest)=line.strip_prefix("atom "){
            let (name,expr)=rest.split_once(':').ok_or_else(||format!("line {}: syntax: atom a: x-y <= 4",index+1))?;
            let name=name.trim(); if !is_name(name) {return Err(format!("line {}: invalid atom name",index+1));}
            declarations.push((name.into(),expr.trim().into()));
        }else if let Some(rest)=line.strip_prefix("clause "){raw_clauses.push((index+1,rest.into()));}
        else {return Err(format!("line {}: expected var, atom, or clause",index+1));}
    }
    if declarations.is_empty(){return Err("at least one atom is required".into());}
    if declarations.len()>200 || vars.len()>100 {return Err("maximum 200 atoms and 100 integer variables".into());}
    let mut atoms=vec![]; let mut lookup=HashMap::new();
    for (name,expr) in declarations {
        if lookup.contains_key(&name) {return Err(format!("duplicate atom {name}"));}
        let (coeffs,bound)=parse_relation(&expr,&vars)?;
        lookup.insert(name.clone(),(atoms.len()+1) as i32);
        atoms.push(Atom{name,coeffs,bound,original:expr});
    }
    let mut clauses=vec![];
    for (ln,raw) in raw_clauses {
        let mut clause=vec![];
        for token in raw.split_whitespace(){
            let negative=token.starts_with('!'); let name=token.trim_start_matches('!');
            let lit=*lookup.get(name).ok_or_else(||format!("line {ln}: unknown atom {name}"))?;
            clause.push(if negative {-lit}else{lit});
        }
        clauses.push(clause);
    }
    Ok(Problem{vars,atoms,clauses})
}
fn is_name(s:&str)->bool {let mut it=s.bytes();matches!(it.next(),Some(b'a'..=b'z'|b'A'..=b'Z'|b'_')) && it.all(|c|c.is_ascii_alphanumeric()||c==b'_')}
#[derive(Clone)]
struct Constraint { coeffs:Vec<i128>, bound:i128 }
fn active_constraints(problem:&Problem, assignments:&[Option<bool>])->Vec<Constraint>{
    problem.atoms.iter().enumerate().filter_map(|(i,atom)|assignments[i].map(|value|{
        if value {Constraint{coeffs:atom.coeffs.clone(),bound:atom.bound}}
        else {Constraint{coeffs:atom.coeffs.iter().map(|v|-v).collect(),bound:-atom.bound-1}}
    })).collect()
}
// A difference constraint has coefficients +1*x -1*y (or just one unit coefficient).
// Bellman-Ford detects negative cycles and provides an exact integer witness.
fn difference_solve(problem:&Problem, cons:&[Constraint])->Option<Option<Vec<i64>>>{
    let n=problem.vars.len(); let mut edges:Vec<(usize,usize,i128)>=vec![];
    for c in cons {
        let mut plus=None;let mut minus=None;
        for (i,&v) in c.coeffs.iter().enumerate(){
            if v==1 && plus.is_none(){plus=Some(i+1);} else if v == -1 && minus.is_none(){minus=Some(i+1);} else if v!=0{return None;}
        }
        edges.push((minus.unwrap_or(0),plus.unwrap_or(0),c.bound));
    }
    for (i,(_,lo,hi)) in problem.vars.iter().enumerate(){edges.push((0,i+1,*hi as i128));edges.push((i+1,0,-(*lo as i128)));}
    let mut dist=vec![0_i128;n+1];
    for k in 0..=n {
        let mut changed=false;
        for &(from,to,weight) in &edges {
            if dist[to]>dist[from]+weight {dist[to]=dist[from]+weight;changed=true;}
        }
        if !changed {break;}
        if k==n {return Some(None);}
    }
    let shift=dist[0];
    let mut model=vec![];
    for &value in dist.iter().skip(1){model.push(i64::try_from(value-shift).ok()?);}
    Some(Some(model))
}
fn bounds_consistent(cons:&[Constraint], mins:&[i64], maxs:&[i64])->bool{
    cons.iter().all(|c|{
        let smallest:i128=c.coeffs.iter().enumerate().map(|(i,&a)|a*if a>=0 {mins[i] as i128}else{maxs[i] as i128}).sum();
        smallest<=c.bound
    })
}
fn theory_search(problem:&Problem, cons:&[Constraint],deadline:Option<Instant>)->Result<Option<Vec<i64>>,()> {
    if let Some(out)=difference_solve(problem,cons){return Ok(out);}
    let mut mins:Vec<i64>=problem.vars.iter().map(|v|v.1).collect();
    let mut maxs:Vec<i64>=problem.vars.iter().map(|v|v.2).collect();
    fn dfs(cons:&[Constraint],mins:&mut[i64],maxs:&mut[i64],deadline:Option<Instant>,visits:&mut u64)->Result<Option<Vec<i64>>,()>{
        *visits+=1;
        if *visits%1024==0 && deadline.is_some_and(|d|Instant::now()>=d){return Err(());}
        if !bounds_consistent(cons,mins,maxs){return Ok(None);}
        let ix=(0..mins.len()).filter(|&i|mins[i]<maxs[i]).min_by_key(|&i| (maxs[i] as i128-mins[i] as i128));
        let Some(ix)=ix else{return Ok(Some(mins.to_vec()));};
        let lo=mins[ix];let hi=maxs[ix];let mid=(lo as i128+(hi as i128-lo as i128)/2) as i64;
        maxs[ix]=mid;
        if let Some(model)=dfs(cons,mins,maxs,deadline,visits)? {maxs[ix]=hi;return Ok(Some(model));}
        maxs[ix]=hi;mins[ix]=mid+1;
        let res=dfs(cons,mins,maxs,deadline,visits);
        mins[ix]=lo;res
    }
    dfs(cons,&mut mins,&mut maxs,deadline,&mut 0)
}
fn append(trace:&mut Vec<String>, enabled:bool,text:String){if enabled && trace.len()<5000 {trace.push(text);}}
pub fn solve(problem:&Problem, timeout:Duration, verbose:bool)->Answer {
    let start=Instant::now();let deadline=start.checked_add(timeout);
    let mut cnf=Cnf{variables:problem.atoms.len(),clauses:problem.clauses.clone()};
    let mut iterations=0;let mut theory_conflicts=0;let mut decisions=0;let mut conflicts=0;
    let mut trace=vec![];let mut events=vec![];let mut truncated=false;
    append(&mut trace,verbose,format!("START: {} atoms, {} clauses, {} integer variables",problem.atoms.len(),cnf.clauses.len(),problem.vars.len()));
    info_event(&mut events,&mut truncated,verbose,"start","smt",
        format!("Pornesc CDCL(T): {} atomi, {} clauze, {} variabile intregi",problem.atoms.len(),cnf.clauses.len(),problem.vars.len()),0,0,0,vec![]);
    loop {
        if deadline.is_some_and(|d|Instant::now()>=d) {
            return answer(Verdict::Unknown,vec![],vec![],iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start);
        }
        iterations+=1;
        info_event(&mut events,&mut truncated,verbose,"iteration","smt",
            format!("Iteratia SMT #{iterations}: CDCL cauta un model al abstractiei booleene ({} clauze)",cnf.clauses.len()),iterations,decisions,conflicts,vec![]);
        let mut opts=Options::default();
        opts.timeout=Some(deadline.map(|d|d.saturating_duration_since(Instant::now())).unwrap_or(timeout));
        opts.capture_steps=verbose;
        let mut solver=Solver::new(&cnf,opts);
        let sat=solver.solve();
        add_cdcl_events(&mut events,&mut truncated,verbose,iterations,decisions,conflicts,&solver);
        decisions+=solver.stats.decisions;conflicts+=solver.stats.conflicts;
        match sat {
            Status::Unsat=>{
                append(&mut trace,verbose,"CDCL: boolean abstraction UNSAT (including learned theory lemmas)".into());
                return answer(Verdict::Unsat,vec![],vec![],iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start);
            }
            Status::Unknown=>return answer(Verdict::Unknown,vec![],vec![],iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start),
            Status::Sat(model)=>{
                let assigned:Vec<Option<bool>>=problem.atoms.iter().enumerate().map(|(i,_)|Some(model[i+1])).collect();
                let cons=active_constraints(problem,&assigned);
                append(&mut trace,verbose,format!("CDCL iteration {iterations}: candidate [{}]",problem.atoms.iter().enumerate().map(|(i,a)|format!("{}={}",a.name,model[i+1])).collect::<Vec<_>>().join(", ")));
                info_event(&mut events,&mut truncated,verbose,"theory_check","theory",
                    "Model boolean gasit. Verific acum constrangerile intregi (LIA / DT).".into(),iterations,decisions,conflicts,vec![]);
                if verbose {
                    for (i,a) in problem.atoms.iter().enumerate() {
                        let meaning=if model[i+1]{a.original.clone()}else{format!("NOT ({})",a.original)};
                        info_event(&mut events,&mut truncated,true,"atom_check","theory",
                            format!("{} = {} implica: {}",a.name,model[i+1],meaning),iterations,decisions,conflicts,vec![if model[i+1]{(i+1) as i32}else{-((i+1) as i32)}]);
                    }
                }
                match theory_search(problem,&cons,deadline){
                    Err(())=>return answer(Verdict::Unknown,vec![],vec![],iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start),
                    Ok(Some(values))=>{
                        let witness=problem.vars.iter().zip(&values).map(|(v,x)|format!("{}={x}",v.0)).collect::<Vec<_>>().join(", ");
                        append(&mut trace,verbose,format!("THEORY SAT: witness [{witness}]"));
                        info_event(&mut events,&mut truncated,verbose,"theory_sat","theory",
                            format!("Model intreg valid: [{witness}]"),iterations,decisions,conflicts,vec![]);
                        return answer(Verdict::Sat,problem.vars.iter().zip(values).map(|(v,x)|(v.0.clone(),x)).collect(),problem.atoms.iter().enumerate().map(|(i,a)|(a.name.clone(),model[i+1])).collect(),iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start);
                    }
                    Ok(None)=>{
                        theory_conflicts+=1;
                        append(&mut trace,verbose,"THEORY CONFLICT: no integer assignment satisfies selected atoms".into());
                        info_event(&mut events,&mut truncated,verbose,"theory_conflict","theory",
                            "Conflict de teorie: niciun vector de numere intregi din domeniile declarate nu satisface simultan toti atomii activi.".into(),iterations,decisions,conflicts,vec![]);
                        // Minimize the theory conflict core by deletion; each failed check is sound.
                        let mut core:Vec<usize>=(0..problem.atoms.len()).collect();let mut at=0;
                        while at<core.len(){
                            if deadline.is_some_and(|d|Instant::now()>=d){return answer(Verdict::Unknown,vec![],vec![],iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start);}
                            let removed=core[at];
                            let mut temp=vec![None;problem.atoms.len()];
                            for &j in &core {if j!=removed{temp[j]=assigned[j];}}
                            match theory_search(problem,&active_constraints(problem,&temp),deadline){
                                Ok(None)=>{
                                    info_event(&mut events,&mut truncated,verbose,"core_minimize","theory",
                                        format!("Elimin {} din nucleul conflictului: restul constrangerilor raman inconsistente.",problem.atoms[removed].name),iterations,decisions,conflicts,vec![]);
                                    core.remove(at);
                                },
                                Ok(Some(_))=>{
                                    info_event(&mut events,&mut truncated,verbose,"core_minimize","theory",
                                        format!("Pastrez {}: fara aceasta constrangere conflictul dispare.",problem.atoms[removed].name),iterations,decisions,conflicts,vec![]);
                                    at+=1;
                                },
                                Err(())=>return answer(Verdict::Unknown,vec![],vec![],iterations,theory_conflicts,decisions,conflicts,trace,events,truncated,start),
                            }
                        }
                        let lemma:Vec<i32>=core.iter().map(|&j|if model[j+1]{-((j+1) as i32)}else{(j+1) as i32}).collect();
                        let human=lemma.iter().map(|l|{let a=&problem.atoms[l.unsigned_abs() as usize-1].name;if *l<0{format!("!{a}")}else{a.clone()}}).collect::<Vec<_>>().join(" OR ");
                        append(&mut trace,verbose,format!("LEARN: theory lemma [{human}]"));
                        info_event(&mut events,&mut truncated,verbose,"theory_lemma","theory",
                            format!("Invat clauza de teorie ({human}). Modelul boolean incompatibil este exclus."),iterations,decisions,conflicts,lemma.clone());
                        cnf.clauses.push(lemma);
                    }
                }
            }
        }
    }
}
/// Run pure propositional CNF through the same solver and capture every internal CDCL step.
pub fn solve_boolean(cnf:&Cnf, timeout:Duration, verbose:bool)->Answer {
    let start=Instant::now();
    let mut opts=Options::default();opts.timeout=Some(timeout);opts.capture_steps=verbose;
    let mut solver=Solver::new(cnf,opts);
    let result=solver.solve();
    let mut events=vec![];let mut truncated=false;
    add_cdcl_events(&mut events,&mut truncated,verbose,1,0,0,&solver);
    let verdict=match &result {Status::Sat(_)=>Verdict::Sat,Status::Unsat=>Verdict::Unsat,Status::Unknown=>Verdict::Unknown};
    let booleans=match result {Status::Sat(model)=>(1..=cnf.variables).map(|i|(format!("x{i}"),model[i])).collect(),_=>vec![]};
    answer(verdict,vec![],booleans,1,0,solver.stats.decisions,solver.stats.conflicts,vec![],events,truncated,start)
}
fn answer(verdict:Verdict,integers:Vec<(String,i64)>,booleans:Vec<(String,bool)>,
          iterations:usize,theory_conflicts:usize,decisions:u64,conflicts:u64,trace:Vec<String>,
          mut events:Vec<StepEvent>,mut truncated:bool,start:Instant)->Answer {
    let (kind,msg)=match verdict {Verdict::Sat=>("sat","SAT: toate constrangerile sunt satisfacute."),Verdict::Unsat=>("unsat","UNSAT: nu exista model valid."),Verdict::Unknown=>("unknown","UNKNOWN: timpul de executie a fost depasit.")};
    if !events.is_empty(){
        if events.len()>=MAX_DEBUG_STEPS { events.pop(); truncated=true; }
        info_event(&mut events,&mut truncated,true,kind,"result",msg.into(),iterations,decisions,conflicts,vec![]);
    }
    Answer{verdict,integers,booleans,iterations,theory_conflicts,decisions,conflicts,trace,events,events_truncated:truncated,elapsed:start.elapsed()}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(s:&str, expected:Verdict){let p=parse_problem(s).unwrap();let a=solve(&p,Duration::from_secs(4),true);assert_eq!(a.verdict,expected,"{:?}",a.trace);}
    #[test] fn dt_sat(){check("var x -5 5\nvar y -5 5\natom a: x-y <= 2\natom b: y-x <= 1\nclause a\nclause b",Verdict::Sat);}
    #[test] fn dt_unsat(){check("var x -5 5\nvar y -5 5\natom a: x-y <= -1\natom b: y-x <= -1\nclause a\nclause b",Verdict::Unsat);}
    #[test] fn lia_sat(){check("var x 0 8\nvar y 0 8\natom a: 2*x+3*y <= 16\natom b: x+y >= 5\nclause a\nclause b",Verdict::Sat);}
    #[test] fn lia_unsat(){check("var x 0 8\natom a: 2*x <= 3\natom b: x >= 3\nclause a\nclause b",Verdict::Unsat);}
    #[test] fn learned_clause(){let p=parse_problem("var x 0 1\natom a: x<=0\natom b: x>=1\nclause a b\nclause !a b\nclause a !b").unwrap();let a=solve(&p,Duration::from_secs(4),true);assert_eq!(a.verdict,Verdict::Unsat);assert!(a.theory_conflicts>0);}
    #[test] fn negated_atom(){check("var x 0 3\natom a: x<=1\nclause !a",Verdict::Sat);}
}

#[cfg(test)]
mod debugger_tests {
    use super::*;
    use crate::verify_model;

    #[test]
    fn pure_cdcl_records_decisions_conflicts_and_final_verdict() {
        let cnf=Cnf{variables:2,clauses:vec![vec![1,2],vec![-1,2],vec![1,-2],vec![-1,-2]]};
        let a=solve_boolean(&cnf,Duration::from_secs(2),true);
        assert_eq!(a.verdict,Verdict::Unsat);
        assert!(a.events.iter().any(|e|e.kind=="decision"));
        assert!(a.events.iter().any(|e|e.kind=="conflict"));
        assert!(a.events.iter().any(|e|e.kind=="learn"));
        assert_eq!(a.events.last().unwrap().kind,"unsat");
    }
    #[test]
    fn replay_trail_levels_and_final_sat_model() {
        let cnf=Cnf{variables:3,clauses:vec![vec![1,2],vec![-1,3],vec![-2,-3]]};
        let a=solve_boolean(&cnf,Duration::from_secs(2),true);
        assert_eq!(a.verdict,Verdict::Sat);
        assert_eq!(a.events.last().unwrap().kind,"sat");
        for e in &a.events { assert_eq!(e.trail.len(),e.levels.len()); }
        let vals=vec![false].into_iter().chain(a.booleans.iter().map(|v|v.1)).collect::<Vec<_>>();
        assert!(verify_model(&cnf,&vals));
    }
    #[test]
    fn dt_conflict_creates_theory_lemma() {
        let p=parse_problem("var x -2 2\nvar y -2 2\natom a: x-y <= -1\natom b: y-x <= -1\nclause a\nclause b").unwrap();
        let a=solve(&p,Duration::from_secs(2),true);
        assert_eq!(a.verdict,Verdict::Unsat);
        assert!(a.events.iter().any(|e|e.kind=="theory_conflict"));
        assert!(a.events.iter().any(|e|e.kind=="theory_lemma"));
        assert!(a.events.iter().any(|e|e.kind=="iteration" && e.iteration>=2));
    }
    #[test]
    fn integer_sat_trace_has_theory_model() {
        let p=parse_problem("var x 0 4\natom a: 2*x <= 6\nclause a").unwrap();
        let a=solve(&p,Duration::from_secs(2),true);
        assert_eq!(a.verdict,Verdict::Sat);
        assert!(a.events.iter().any(|e|e.kind=="theory_sat"));
        assert_eq!(a.events.last().unwrap().kind,"sat");
    }
}

#[cfg(test)]
mod reynolds_slides_tests {
    use super::*;

    fn atom_true(atom: &Atom, values: &[i64]) -> bool {
        atom.coeffs.iter().zip(values).map(|(a, x)| a * (*x as i128)).sum::<i128>() <= atom.bound
    }
    fn lemma_true(problem: &Problem, lemma: &[i32], values: &[i64]) -> bool {
        lemma.iter().any(|&lit| {
            let truth=atom_true(&problem.atoms[lit.unsigned_abs() as usize - 1], values);
            if lit > 0 { truth } else { !truth }
        })
    }
    #[test]
    fn vtsa_slide_50_unsat_and_theory_lemmas_are_valid() {
        let p=parse_problem(include_str!("../examples/reynolds_slide50.smt")).unwrap();
        let ans=solve(&p, Duration::from_secs(5), true);
        assert_eq!(ans.verdict, Verdict::Unsat);
        let lemmas=ans.events.iter().filter(|e|e.kind=="theory_lemma").collect::<Vec<_>>();
        assert!(!lemmas.is_empty(), "expected a theory conflict clause");
        for e in lemmas {
            for x in -10..=10 {
                for y in -10..=10 {
                    assert!(lemma_true(&p, &e.clause, &[x,y]),
                        "unsound learned lemma {:?} for x={x}, y={y}", e.clause);
                }
            }
        }
    }
    #[test]
    fn vtsa_exercise_unsat_and_lemmas_are_valid() {
        let p=parse_problem(include_str!("../examples/reynolds_exercise.smt")).unwrap();
        let ans=solve(&p, Duration::from_secs(5), true);
        assert_eq!(ans.verdict, Verdict::Unsat);
        let lemmas=ans.events.iter().filter(|e|e.kind=="theory_lemma").collect::<Vec<_>>();
        assert!(!lemmas.is_empty(), "expected theory learning for the exercise");
        for e in lemmas {
            for x in -8..=8 { for y in -8..=8 { for z in -8..=8 {
                assert!(lemma_true(&p, &e.clause, &[x,y,z]),
                    "unsound theory lemma {:?} at ({x},{y},{z})",e.clause);
            }}}
        }
    }
}

// -----------------------------------------------------------------------------
// Interactive DPLL(T) decision checker (uses the SAME exact theory_search as SMT).
// This explores one user-selected branch. It does not pretend that a locally
// consistent branch is a global SAT proof. The returned lemma is the negation
// of an actually inconsistent subset of assigned arithmetic literals.
// -----------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct ManualResult {
    pub status: &'static str,       // OK, SAT, CONFLICT, UNSAT, UNKNOWN
    pub source: &'static str,       // boolean, theory, none
    pub message: String,
    pub trail: Vec<i32>,
    pub levels: Vec<usize>,
    pub propagated: Vec<(i32, Vec<i32>)>,
    pub conflict_clause: Vec<i32>,
    pub conflict_core: Vec<i32>,
    pub lemma: Vec<i32>,
    pub witness: Vec<(String, i64)>,
}

fn manual_blank() -> ManualResult {
    ManualResult {
        status: "OK", source: "none", message: String::new(),
        trail: vec![], levels: vec![], propagated: vec![],
        conflict_clause: vec![], conflict_core: vec![], lemma: vec![], witness: vec![],
    }
}
fn literal_name(problem:&Problem, lit:i32) -> String {
    let name = &problem.atoms[lit.unsigned_abs() as usize - 1].name;
    if lit < 0 {format!("!{name}")} else {name.clone()}
}
fn manual_apply_literal(out:&mut ManualResult, values:&mut[i8], lit:i32, level:usize,
                        reason:Option<&[i32]>) -> bool {
    let id=lit.unsigned_abs() as usize - 1;
    let v=if lit>0 {1} else {-1};
    if values[id] == -v {return false;}
    if values[id] == v {return true;}
    values[id]=v;
    out.trail.push(lit);out.levels.push(level);
    if let Some(clause)=reason {out.propagated.push((lit,clause.to_vec()));}
    true
}
fn manual_boolean_propagate(problem:&Problem, lemmas:&[Vec<i32>],
    values:&mut[i8], level:usize, out:&mut ManualResult) -> bool {
    loop {
        let mut changed=false;
        for clause in problem.clauses.iter().chain(lemmas) {
            let mut open=0; let mut last=0; let mut sat=false;
            for &lit in clause {
                let v=values[lit.unsigned_abs() as usize-1];
                if v==0 {open+=1;last=lit;}
                else if (v==1)==(lit>0) {sat=true;break;}
            }
            if sat {continue;}
            if open==0 {
                out.source="boolean";out.status="CONFLICT";
                out.conflict_clause=clause.clone();
                out.message=format!("Conflict boolean: clauza ({}) este falsă în context.",
                    clause.iter().map(|&l|literal_name(problem,l)).collect::<Vec<_>>().join(" ∨ "));
                return false;
            }
            if open==1 && values[last.unsigned_abs() as usize-1]==0 {
                if !manual_apply_literal(out,values,last,level,Some(clause)) {unreachable!()}
                changed=true;
            }
        }
        if !changed {return true;}
    }
}
fn manual_theory_check(problem:&Problem,values:&[i8],out:&mut ManualResult,
                       deadline:Option<Instant>) {
    let assumptions:Vec<i32>=values.iter().enumerate().filter_map(|(i,&v)|
        if v==0{None}else{Some((i as i32+1)*v as i32)}).collect();
    let mut assignment=vec![None;values.len()];
    for &lit in &assumptions {assignment[lit.unsigned_abs() as usize-1]=Some(lit>0);}
    match theory_search(problem,&active_constraints(problem,&assignment),deadline) {
        Err(()) => {
            out.status="UNKNOWN";out.source="theory";
            out.message="Limită de timp la verificarea LIA/DT. Alegerea nu este clasificată drept conflict.".into();
        }
        Ok(Some(w)) => {
            out.witness=problem.vars.iter().zip(w).map(|(v,x)|(v.0.clone(),x)).collect();
            if values.iter().all(|&v|v!=0) {
                out.status="SAT";
                out.message="Toți atomii sunt atribuiți și există un model întreg pentru acest context.".into();
            } else {
                out.status="OK";
                out.message="Contextul este compatibil cu LIA/DT; mai sunt atomi de decis.".into();
            }
        }
        Ok(None) => {
            // Deletion-based minimal inconsistent subset (not necessarily minimum size).
            // Every generated clause is therefore a VALID theory lemma.
            let mut core=assumptions;
            let mut i=0;
            while i<core.len(){
                let mut test=vec![None;values.len()];
                for (j,&lit) in core.iter().enumerate(){if j!=i {test[lit.unsigned_abs() as usize-1]=Some(lit>0);}}
                match theory_search(problem,&active_constraints(problem,&test),deadline) {
                    Ok(None)=>{core.remove(i);},
                    Ok(Some(_))=>{i+=1;},
                    Err(())=>{ // Keep a sound, possibly non-minimal lemma.
                        break;
                    }
                }
            }
            out.status="CONFLICT";out.source="theory";
            out.lemma=core.iter().map(|&x|-x).collect();
            out.conflict_clause=out.lemma.clone();out.conflict_core=core.clone();
            let names=core.iter().map(|&lit|literal_name(problem,lit)).collect::<Vec<_>>().join(" ∧ ");
            let lemma=out.lemma.iter().map(|&lit|literal_name(problem,lit)).collect::<Vec<_>>().join(" ∨ ");
            out.message=format!("Conflict LIA/DT: {names} nu are soluție întreagă. Lemă validă: ({lemma}).");
        }
    }
}

/// Evaluate Boolean propagation and exact bounded-LIA/DT feasibility under the
/// given ordered decisions, plus an optional trial literal, WITHOUT committing it.
/// Theory lemmas supplied here must have been returned by a previous check.
pub fn manual_check(problem:&Problem, decisions:&[i32], lemmas:&[Vec<i32>],
                    candidate:Option<i32>, timeout:Duration)->Result<ManualResult,String>{
    let n=problem.atoms.len();
    if decisions.len()>n || lemmas.len()>200 {return Err("Too many decisions / lemmas".into());}
    for &lit in decisions.iter().chain(candidate.iter()).chain(lemmas.iter().flatten()) {
        if lit==0 || lit.unsigned_abs() as usize>n{return Err(format!("Invalid atom literal {lit}"));}
    }
    if lemmas.iter().any(|c|c.len()>n) {return Err("Invalid lemma length".into());}
    let deadline=Instant::now().checked_add(timeout);
    let mut values=vec![0_i8;n];let mut out=manual_blank();
    if !manual_boolean_propagate(problem,lemmas,&mut values,0,&mut out) {
        out.status="UNSAT";
        return Ok(out);
    }
    // When a branch is replayed after a new lemma, the lemma may force its old
    // decision. In that case, a contradictory replay is reported honestly.
    for (i,&lit) in decisions.iter().enumerate(){
        if !manual_apply_literal(&mut out,&mut values,lit,i+1,None) {
            out.status="CONFLICT";out.source="boolean";
            out.message=format!("Decizia {} contrazice propagarea booleană curentă.",literal_name(problem,lit));
            return Ok(out);
        }
        if !manual_boolean_propagate(problem,lemmas,&mut values,i+1,&mut out){return Ok(out);}
    }
    if let Some(lit)=candidate {
        if values[lit.unsigned_abs() as usize-1]!=0 {
            out.status="CONFLICT";out.source="boolean";
            out.message=format!("{} este deja fixat la valoarea opusă (unit propagation).",literal_name(problem,lit));
            return Ok(out);
        }
        manual_apply_literal(&mut out,&mut values,lit,decisions.len()+1,None);
        if !manual_boolean_propagate(problem,lemmas,&mut values,decisions.len()+1,&mut out){return Ok(out);}
    }
    manual_theory_check(problem,&values,&mut out,deadline);
    if candidate.is_none() && decisions.is_empty() && out.status=="CONFLICT" {
        // Contextul de nivel 0 este incompatibil in teoria aritmeticii.
        out.status="UNSAT";
    }
    Ok(out)
}

#[cfg(test)]
mod manual_tests {
    use super::*;
    const EXAMPLE:&str="var x -10 10\nvar y -10 10\natom a: x+1 > 0\natom b: x+y > 0\natom c: x < 0\natom d: x+y > 4\nclause a b\nclause c d\nclause !b\n";
    #[test]
    fn exact_names_and_forced_propagation(){
        let p=parse_problem(EXAMPLE).unwrap();
        assert_eq!(p.atoms.iter().map(|a|a.name.as_str()).collect::<Vec<_>>(),vec!["a","b","c","d"]);
        let initial=manual_check(&p,&[],&[],None,Duration::from_secs(2)).unwrap();
        assert_eq!(initial.status,"OK");
        assert!(initial.trail.contains(&-2)&&initial.trail.contains(&1));
    }
    #[test]
    fn slide_theory_conflict_c_and_theory_lemma(){
        let p=parse_problem(EXAMPLE).unwrap();
        let r=manual_check(&p,&[],&[],Some(3),Duration::from_secs(2)).unwrap();
        assert_eq!(r.source,"theory");
        assert_eq!(r.status,"CONFLICT");
        assert_eq!(r.lemma,vec![-1,-3]);
    }
    #[test]
    fn slide_theory_conflict_d_and_theory_lemma(){
        let p=parse_problem(EXAMPLE).unwrap();
        let r=manual_check(&p,&[],&[],Some(4),Duration::from_secs(2)).unwrap();
        assert_eq!(r.source,"theory");
        assert_eq!(r.status,"CONFLICT");
        assert_eq!(r.lemma,vec![2,-4]);
    }
    #[test]
    fn learning_closes_example_without_invented_atoms(){
        let p=parse_problem(EXAMPLE).unwrap();
        let r=manual_check(&p,&[],&[vec![-1,-3],vec![2,-4]],None,Duration::from_secs(2)).unwrap();
        assert_eq!(r.status,"UNSAT");
        assert_eq!(r.source,"boolean");
    }
    #[test]
    fn valid_branch_is_green_not_global_sat(){
        let p=parse_problem("var x 0 3\natom a: x>=1\natom b: x>=2\nclause a b\n").unwrap();
        let r=manual_check(&p,&[],&[],Some(1),Duration::from_secs(2)).unwrap();
        assert_eq!(r.status,"OK");
    }
}

/// True iff negating this clause has NO bounded integer model. It is a
/// standalone theory check: Boolean CNF constraints are deliberately ignored.
pub fn validate_theory_lemma(problem:&Problem,clause:&[i32],timeout:Duration)->Result<bool,String>{
    let mut assignment=vec![None;problem.atoms.len()];
    for &lit in clause {
        if lit==0 || lit.unsigned_abs() as usize>problem.atoms.len(){return Err("Invalid theory lemma".into());}
        let id=lit.unsigned_abs() as usize-1;
        let truth=lit<0;
        if assignment[id].is_some_and(|v|v!=truth) {return Ok(true);} // tautology
        assignment[id]=Some(truth);
    }
    let deadline=Instant::now().checked_add(timeout);
    match theory_search(problem,&active_constraints(problem,&assignment),deadline){
        Ok(None)=>Ok(true),
        Ok(Some(_))=>Ok(false),
        Err(())=>Err("Theory lemma validation timed out".into()),
    }
}

#[cfg(test)]
mod more_manual_tests {
    use super::*;
    #[test]
    fn valid_then_invalid_click_decisions(){
        let p=parse_problem("var x 0 5\natom a: x>=2\natom b: x<=3\natom c: x>=4\nclause a b\n").unwrap();
        let yes=manual_check(&p,&[],&[],Some(2),Duration::from_secs(2)).unwrap();
        assert_eq!(yes.status,"OK");
        let no=manual_check(&p,&[2],&[],Some(3),Duration::from_secs(2)).unwrap();
        assert_eq!(no.status,"CONFLICT");
        assert_eq!(no.source,"theory");
        assert_eq!(no.lemma,vec![-2,-3]);
        assert!(validate_theory_lemma(&p,&no.lemma,Duration::from_secs(2)).unwrap());
        let yes=manual_check(&p,&[2],&[],Some(-3),Duration::from_secs(2)).unwrap();
        assert_eq!(yes.status,"OK");
    }
}

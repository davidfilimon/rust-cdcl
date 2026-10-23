//! CLI + tiny dependency-free local HTTP server for the CDCL(T) step debugger.
use cdcl_rust::smt::{parse_problem, solve, solve_boolean, manual_check, ManualResult, Answer, Problem, Verdict};
use cdcl_rust::parse_dimacs;
use std::{env, fs, io::{Read,Write}, net::{TcpListener,TcpStream}, time::Duration};

const PAGE: &str = include_str!("../../web/index.html");

fn escape(s: &str) -> String {
    let mut out=String::new();
    for ch in s.chars() {
        match ch {
            '"'=>out.push_str("\\\""),
            '\\'=>out.push_str("\\\\"),
            '\n'=>out.push_str("\\n"),
            '\r'=>out.push_str("\\r"),
            '\t'=>out.push_str("\\t"),
            c if c<' '=>out.push_str(&format!("\\u{:04x}",c as u32)),
            c=>out.push(c),
        }
    }
    out
}
fn quoted(s:&str)->String { format!("\"{}\"",escape(s)) }
fn ints(nums:&[i32])->String {
    format!("[{}]",nums.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(","))
}
fn sizes(nums:&[usize])->String {
    format!("[{}]",nums.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(","))
}
fn json(a:&Answer, problem:&Problem,mode:&str)->String {
    let status=match a.verdict{Verdict::Sat=>"SAT",Verdict::Unsat=>"UNSAT",Verdict::Unknown=>"UNKNOWN"};
    let mut s=format!("{{\"status\":{},\"mode\":{},\"iterations\":{},\"theory_conflicts\":{},\"decisions\":{},\"conflicts\":{},\"elapsed_ms\":{},\"truncated\":{},\"model\":{{",
        quoted(status),quoted(mode),a.iterations,a.theory_conflicts,a.decisions,a.conflicts,a.elapsed.as_millis(),a.events_truncated);
    for (i,(name,n)) in a.integers.iter().enumerate(){if i>0{s.push(',');}s+=&format!("{}:{}",quoted(name),n);}
    s.push_str("},\"atoms\":{");
    for (i,(name,b)) in a.booleans.iter().enumerate(){if i>0{s.push(',');}s+=&format!("{}:{}",quoted(name),b);}
    s.push_str("},\"variables\":[");
    for (i,(name,lo,hi)) in problem.vars.iter().enumerate() {
        if i>0{s.push(',');}
        s+=&format!("{{\"name\":{},\"min\":{},\"max\":{}}}",quoted(name),lo,hi);
    }
    s.push_str("],\"definitions\":[");
    for (i,atom) in problem.atoms.iter().enumerate() {
        if i>0{s.push(',');}
        s+=&format!("{{\"name\":{},\"expression\":{}}}",quoted(&atom.name),quoted(&atom.original));
    }
    s.push_str("],\"clauses\":[");
    for (i,clause) in problem.clauses.iter().enumerate() {
        if i>0{s.push(',');}
        s+=&ints(clause);
    }
    s.push_str("],\"events\":[");
    for (i,e) in a.events.iter().enumerate() {
        if i>0{s.push(',');}
        s+=&format!("{{\"kind\":{},\"source\":{},\"message\":{},\"iteration\":{},\"level\":{},\"trail\":{},\"levels\":{},\"clause\":{},\"decisions\":{},\"conflicts\":{}}}",
            quoted(e.kind),quoted(e.source),quoted(&e.message),e.iteration,e.level,ints(&e.trail),sizes(&e.levels),ints(&e.clause),e.decisions,e.conflicts);
    }
    s.push_str("],\"trace\":[");
    for (i,t) in a.trace.iter().enumerate(){if i>0{s.push(',');}s+=&quoted(t);}
    s.push_str("]}");s
}

fn split_signed(raw:&str)->Result<Vec<i32>,String>{
    if raw.trim().is_empty(){return Ok(vec![])}
    raw.split(',').map(|part|part.trim().parse::<i32>()
        .map_err(|_|format!("Invalid literal in manual state: {part}"))).collect()
}
fn encode_manual(result:&ManualResult)->String {
    let mut s=format!("{{\"status\":{},\"source\":{},\"message\":{},\"trail\":{},\"levels\":{},\"clause\":{},\"core\":{},\"lemma\":{},\"propagated\":[",
        quoted(result.status),quoted(result.source),quoted(&result.message),ints(&result.trail),sizes(&result.levels),
        ints(&result.conflict_clause),ints(&result.conflict_core),ints(&result.lemma));
    for (i,(lit,reason)) in result.propagated.iter().enumerate(){
        if i>0{s.push(',')};
        s+=&format!("{{\"literal\":{},\"clause\":{}}}",lit,ints(reason));
    }
    s.push_str("],\"witness\":{");
    for (i,(name,value)) in result.witness.iter().enumerate(){
        if i>0{s.push(',');}
        s+=&format!("{}:{}",quoted(name),value);
    }
    s.push_str("}}");s
}
fn manual_api(source:&str, choices:&str,lemma_text:&str,candidate:&str)->Result<String,String>{
    if is_dimacs(source) {return Err("Deciziile manuale sunt disponibile pentru formule SMT cu atom a:, b:, ...; pentru CNF folosește modul replay.".into());}
    let problem=parse_problem(source)?;
    let decisions=split_signed(choices)?;
    let lemmas:Vec<Vec<i32>>=if lemma_text.trim().is_empty(){vec![]}
        else {lemma_text.split(';').map(split_signed).collect::<Result<_,_>>()?};
    if lemmas.iter().any(|c|c.is_empty()) {return Err("Empty manual lemma".into());}
    let trial=if candidate.trim().is_empty(){None}else{Some(candidate.trim().parse::<i32>().map_err(|_|"Invalid candidate literal")?)};
    // Never silently trust an arbitrary client lemma: check each against the
    // arithmetic theory, even when editing request headers by hand.
    for clause in &lemmas {
        let mut assignment=vec![None;problem.atoms.len()];
        for &lit in clause {
            if lit==0 || lit.unsigned_abs() as usize>problem.atoms.len(){return Err("Invalid theory lemma atom".into());}
            let ix=lit.unsigned_abs() as usize-1;
            let required=lit<0;
            if let Some(previous)=assignment[ix] {if previous!=required{return Err("Tautological lemma not needed".into());}}
            assignment[ix]=Some(required);
        }
        // Verifying a proposed lemma by invoking manual_check on negated
        // literals alone would also include Boolean constraints. We need a
        // genuine theory-validity check for each standalone clause instead.
        if !cdcl_rust::smt::validate_theory_lemma(&problem,clause,Duration::from_millis(500))? {
            return Err("Rejected: lemma is not valid in LIA/DT".into());
        }
    }
    let base=manual_check(&problem,&decisions,&lemmas,None,Duration::from_millis(700))?;
    let attempted=if let Some(lit)=trial {
        manual_check(&problem,&decisions,&lemmas,Some(lit),Duration::from_millis(700))?
    }else{base.clone()};
    let mut s=format!("{{\"mode\":\"MANUAL\",\"base\":{},\"attempt\":{},\"definitions\":[",encode_manual(&base),encode_manual(&attempted));
    for (i,a) in problem.atoms.iter().enumerate(){
        if i>0{s.push(',');}
        s+=&format!("{{\"name\":{},\"expression\":{}}}",quoted(&a.name),quoted(&a.original));
    }
    s.push_str("],\"clauses\":[");
    for (i,c) in problem.clauses.iter().enumerate(){if i>0{s.push(',');}s+=&ints(c);}
    s.push_str("],\"options\":[");
    if matches!(base.status,"OK"|"SAT") && problem.atoms.len()<=30 {
        let assigned:std::collections::HashSet<i32>=base.trail.iter().map(|l|l.abs()).collect();
        let mut first=true;
        for index in 1..=problem.atoms.len() as i32 {
            if assigned.contains(&index) {continue;}
            for literal in [index,-index] {
                let preview=manual_check(&problem,&decisions,&lemmas,Some(literal),Duration::from_millis(250))?;
                if !first{s.push(',')}first=false;
                s+=&format!("{{\"literal\":{},\"status\":{},\"reason\":{}}}",literal,quoted(preview.status),quoted(&preview.message));
            }
        }
    }
    s.push_str("]}");Ok(s)
}

fn is_dimacs(text:&str)->bool {
    text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('c') && !line.starts_with('%'))
        .next().is_some_and(|line|line.starts_with("p cnf "))
}
fn run(text:&str,seconds:f64,debug:bool)->Result<String,String>{
    let timeout=Duration::from_secs_f64(seconds);
    if is_dimacs(text) {
        let cnf=parse_dimacs(text)?;
        if cnf.variables>300 || cnf.clauses.len()>4000 { return Err("Debugger limit: max 300 variables and 4000 clauses; use the CLI for larger benchmarks".into()); }
        let p=Problem{
            vars:vec![],
            atoms:(1..=cnf.variables).map(|i|cdcl_rust::smt::Atom{name:format!("x{i}"),coeffs:vec![],bound:0,original:"Boolean variable".into()}).collect(),
            clauses:cnf.clauses.clone(),
        };
        return Ok(json(&solve_boolean(&cnf,timeout,debug),&p,"CNF"));
    }
    let p=parse_problem(text)?;
    Ok(json(&solve(&p,timeout,debug),&p,"SMT"))
}
fn response(stream:&mut TcpStream,code:&str,kind:&str,body:&str) {
    let msg=format!("HTTP/1.1 {code}\r\nContent-Type: {kind}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n{body}",body.len());
    let _=stream.write_all(msg.as_bytes());
}
fn handle(mut stream:TcpStream){
    let _=stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut data=vec![0_u8;8192];let mut used=0;
    let head_end=loop{
        if used>=data.len(){return response(&mut stream,"413 Payload Too Large","text/plain","Request header too large");}
        match stream.read(&mut data[used..]){Ok(0)|Err(_)=>return,Ok(n)=>used+=n}
        if let Some(i)=data[..used].windows(4).position(|w|w==b"\r\n\r\n"){break i+4;}
    };
    let header=String::from_utf8_lossy(&data[..head_end]);
    let mut lines=header.lines();let request=lines.next().unwrap_or("");
    if request.starts_with("GET / ") || request.starts_with("GET /index.html ") {
        return response(&mut stream,"200 OK","text/html",PAGE);
    }
    let manual=request.starts_with("POST /api/manual ");
    if !manual && !request.starts_with("POST /api/solve ") {
        return response(&mut stream,"404 Not Found","text/plain","Not found");
    }
    let all_headers:Vec<_>=lines.collect();
    let header_value=|key:&str| -> &str { all_headers.iter().filter_map(|l|l.split_once(':')).find(|(k,_)|k.eq_ignore_ascii_case(key)).map(|(_,v)|v.trim()).unwrap_or("") };
    let len:usize=all_headers.iter().filter_map(|l|l.split_once(':'))
        .find(|(k,_)|k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_,v)|v.trim().parse().ok()).unwrap_or(0);
    if len==0 || len>65536 {
        return response(&mut stream,"413 Payload Too Large","application/json","{\"error\":\"Expected Content-Length 1..65536\"}");
    }
    let mut body=Vec::with_capacity(len);
    body.extend_from_slice(&data[head_end..used]);body.truncate(len);
    while body.len()<len {
        let mut temp=[0_u8;4096];
        match stream.read(&mut temp[..(len-body.len()).min(4096)]){
            Ok(0)|Err(_)=>return,Ok(n)=>body.extend_from_slice(&temp[..n]),
        }
    }
    let text=match String::from_utf8(body){
        Ok(s)=>s,
        Err(_)=>return response(&mut stream,"400 Bad Request","application/json","{\"error\":\"Invalid UTF-8\"}"),
    };
    let result=if manual {manual_api(&text,header_value("x-manual-decisions"),header_value("x-manual-lemmas"),header_value("x-manual-candidate"))}
        else {run(&text,8.0,true)};
    match result {
        Ok(json)=>response(&mut stream,"200 OK","application/json",&json),
        Err(e)=>response(&mut stream,"400 Bad Request","application/json",&format!("{{\"error\":{}}}",quoted(&e))),
    }
}
fn main(){
    let args:Vec<String>=env::args().collect();
    if args.get(1).is_some_and(|s|s=="web"){
        let port=args.get(2).map(String::as_str).unwrap_or("7878");
        let addr=format!("127.0.0.1:{port}");
        let listener=TcpListener::bind(&addr).unwrap_or_else(|e|panic!("Failed to bind {addr}: {e}"));
        println!("CDCL(T) debugger: http://{addr} (Ctrl+C to stop)");
        for stream in listener.incoming(){if let Ok(s)=stream{handle(s);}}
        return;
    }
    let Some(path)=args.get(1) else {
        eprintln!("Usage: cargo run --release --bin smt -- FILE.smt [--trace] [--tmax 5]\\n       cargo run --release --bin smt -- web [port]");
        std::process::exit(2)
    };
    let mut trace=false;let mut seconds=30.0;
    let mut i=2;
    while i<args.len() {
        match args[i].as_str(){
            "--trace"=>trace=true,
            "--tmax"=>{i+=1;seconds=args.get(i).and_then(|s|s.parse::<f64>().ok()).unwrap_or(-1.0);},
            s=>{eprintln!("Unknown option: {s}");std::process::exit(2)},
        }
        i+=1;
    }
    if !seconds.is_finite() || seconds<=0.0 || seconds>3600.0 {
        eprintln!("--tmax must be in (0,3600]");std::process::exit(2);
    }
    let text=fs::read_to_string(path).unwrap_or_else(|e|{eprintln!("{e}");std::process::exit(2)});
    let a=if is_dimacs(&text){
        let p=parse_dimacs(&text).unwrap_or_else(|e|{eprintln!("{e}");std::process::exit(2)});
        solve_boolean(&p,Duration::from_secs_f64(seconds),trace)
    }else{
        let p=parse_problem(&text).unwrap_or_else(|e|{eprintln!("{e}");std::process::exit(2)});
        solve(&p,Duration::from_secs_f64(seconds),trace)
    };
    for line in &a.trace{println!("c {line}");}
    if trace {for e in &a.events {println!("c [{} / iter {} / L{}] {}",e.kind,e.iteration,e.level,e.message);}}
    println!("s {}",match a.verdict{Verdict::Sat=>"SATISFIABLE",Verdict::Unsat=>"UNSATISFIABLE",Verdict::Unknown=>"UNKNOWN"});
    for (name,x) in &a.integers{println!("v {name} = {x}");}
    println!("c CDCL iterations: {} | theory conflicts: {} | decisions: {} | SAT conflicts: {} | ms: {}",a.iterations,a.theory_conflicts,a.decisions,a.conflicts,a.elapsed.as_millis());
}

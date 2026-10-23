use cdcl_rust::{nqueens, parse_dimacs, pigeonhole, random_ksat, verify_model, write_dimacs,
    Cnf, Options, Solver, Stats, Status};
use std::{env, fs, io::{self, Read, Write}, path::Path, process, time::Duration};

fn usage() -> &'static str {
"CDCL SAT solver (Rust, no dependencies)
Usage:
  cargo run --release -- solve FILE.cnf [--tmax SECONDS] [--max-conflicts N]
                     [--trace] [--no-restarts] [--no-reduce]
                     [--heuristic vsids|first] [--check-model]
  cargo run --release -- bench DIRECTORY [--tmax SECONDS] [--csv results.csv]
  cargo run --release -- generate random3 --n 20 --m 85 --seed 42 --output test.cnf
  cargo run --release -- generate pigeonhole --n 4 --output php.cnf
  cargo run --release -- generate queens --n 8 --output queens.cnf

  'solve -' reads DIMACS from standard input.
  Tmax = maximum wall-clock runtime in seconds. UNKNOWN = timeout or conflict limit.
  Exit codes for solve: 10 SAT, 20 UNSAT, 0 UNKNOWN, 2 input/CLI error.
"
}
fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("Error: {message}\n\n{}", usage());
    process::exit(2)
}
fn value_after<'a>(args: &'a [String], at: &mut usize, flag: &str) -> &'a str {
    *at += 1;
    args.get(*at).unwrap_or_else(|| fail(format!("missing value after {flag}")))
}
fn parse_options(args: &[String], is_bench: bool) -> (Options, Option<String>, bool) {
    let mut opts = Options::default();
    let mut csv = None;
    let mut check_model = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tmax" => {
                let s = value_after(args,&mut i,"--tmax");
                let x: f64 = s.parse().unwrap_or_else(|_| fail("--tmax must be a nonnegative number"));
                if !x.is_finite() || x < 0.0 || x > 1e9 { fail("invalid --tmax value"); }
                opts.timeout = Some(Duration::from_secs_f64(x));
            }
            "--max-conflicts" => {
                let s = value_after(args,&mut i,"--max-conflicts");
                opts.max_conflicts = Some(s.parse().unwrap_or_else(|_| fail("invalid --max-conflicts")));
            }
            "--trace" => opts.trace = true,
            "--no-restarts" => opts.enable_restarts = false,
            "--no-reduce" => opts.enable_reduction = false,
            "--heuristic" => {
                match value_after(args,&mut i,"--heuristic") {
                    "vsids" => opts.vsids = true,
                    "first" => opts.vsids = false,
                    _ => fail("--heuristic must be vsids or first"),
                }
            }
            "--csv" if is_bench => csv = Some(value_after(args,&mut i,"--csv").to_string()),
            "--check-model" if !is_bench => check_model = true,
            other => fail(format!("unknown option: {other}")),
        }
        i += 1;
    }
    (opts, csv, check_model)
}
fn load(path: &str) -> Result<Cnf, String> {
    let mut content = String::new();
    if path == "-" { io::stdin().read_to_string(&mut content).map_err(|e| e.to_string())?; }
    else { content = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?; }
    parse_dimacs(&content)
}
fn status_name(status: &Status) -> &'static str {
    match status { Status::Sat(_) => "SAT", Status::Unsat => "UNSAT", Status::Unknown => "UNKNOWN" }
}
fn print_stats(s: &Stats) {
    println!("c decisions        : {}",s.decisions);
    println!("c propagations     : {}",s.propagations);
    println!("c conflicts        : {}",s.conflicts);
    println!("c learned clauses  : {}",s.learned_clauses);
    println!("c deleted clauses  : {}",s.deleted_clauses);
    println!("c restarts         : {}",s.restarts);
    println!("c backjumps        : {}",s.backjumps);
    println!("c elapsed seconds  : {:.6}",s.elapsed.as_secs_f64());
}
fn solve_one(path: &str, args: &[String]) -> ! {
    let (opts, _, check_model) = parse_options(args, false);
    let cnf = load(path).unwrap_or_else(|e| fail(e));
    let mut solver = Solver::new(&cnf, opts);
    let result = solver.solve();
    println!("c variables        : {}", cnf.variables);
    println!("c original clauses : {}", cnf.clauses.len());
    print_stats(&solver.stats);
    match result {
        Status::Sat(model) => {
            if check_model && !verify_model(&cnf, &model) { fail("INTERNAL ERROR: invalid satisfying assignment"); }
            println!("s SATISFIABLE");
            print!("v");
            for v in 1..=cnf.variables {
                print!(" {}", if model[v] { v as i32 } else { -(v as i32) });
                if v % 15 == 0 && v < cnf.variables { print!(" 0\nv"); }
            }
            println!(" 0");
            process::exit(10)
        }
        Status::Unsat => { println!("s UNSATISFIABLE"); process::exit(20); }
        Status::Unknown => { println!("s UNKNOWN"); process::exit(0); }
    }
}
fn bench(dir: &str, args: &[String]) {
    let (opts, csv, _) = parse_options(args, true);
    let mut paths = Vec::new();
    for item in fs::read_dir(dir).unwrap_or_else(|e| fail(e)) {
        let item = item.unwrap_or_else(|e| fail(e));
        if item.path().extension().is_some_and(|e| e=="cnf") { paths.push(item.path()); }
    }
    paths.sort();
    if paths.is_empty() { fail("benchmark directory has no .cnf files"); }
    let mut output = "file,vars,clauses,status,seconds,decisions,propagations,conflicts,learned,deleted,restarts,backjumps\n".to_string();
    println!("{:<28} {:>8} {:>10} {:>9} {:>12}","File","Vars","Clauses","Result","Seconds");
    for path in paths {
        let cnf = load(path.to_str().unwrap_or("")).unwrap_or_else(|e| fail(e));
        let mut solver = Solver::new(&cnf, opts.clone());
        let result = solver.solve();
        let s = &solver.stats;
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        println!("{:<28} {:>8} {:>10} {:>9} {:>12.6}",filename,cnf.variables,cnf.clauses.len(),status_name(&result),s.elapsed.as_secs_f64());
        let quoted = format!("\"{}\"", filename.replace('"', "\"\""));
        output.push_str(&format!("{quoted},{},{},{},{:.6},{},{},{},{},{},{},{}\n",
            cnf.variables,cnf.clauses.len(),status_name(&result),s.elapsed.as_secs_f64(),
            s.decisions,s.propagations,s.conflicts,s.learned_clauses,s.deleted_clauses,s.restarts,s.backjumps));
    }
    if let Some(csv_path) = csv {
        fs::write(&csv_path, output).unwrap_or_else(|e| fail(e));
        println!("CSV written: {csv_path}");
    }
}
fn generate(kind: &str, args: &[String]) {
    let mut n: Option<usize> = None;
    let mut m: Option<usize> = None;
    let mut seed: u64 = 42;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--n" => n = Some(value_after(args,&mut i,"--n").parse().unwrap_or_else(|_| fail("invalid --n"))),
            "--m" => m = Some(value_after(args,&mut i,"--m").parse().unwrap_or_else(|_| fail("invalid --m"))),
            "--seed" => seed = value_after(args,&mut i,"--seed").parse().unwrap_or_else(|_| fail("invalid --seed")),
            "--output" => output = Some(value_after(args,&mut i,"--output").to_owned()),
            other => fail(format!("unknown generator option: {other}")),
        }
        i += 1;
    }
    let n = n.unwrap_or_else(|| fail("generator requires --n"));
    let cnf = match kind {
        "random3" => random_ksat(n, m.unwrap_or_else(|| fail("random3 requires --m")),3,seed),
        "pigeonhole" => pigeonhole(n),
        "queens" => nqueens(n),
        _ => fail(format!("unknown generator: {kind}")),
    }.unwrap_or_else(|e| fail(e));
    let s = write_dimacs(&cnf);
    if let Some(path) = output {
        let p = Path::new(&path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() { fs::create_dir_all(parent).unwrap_or_else(|e| fail(e)); }
        }
        fs::write(&path,s).unwrap_or_else(|e| fail(e));
        eprintln!("generated {path} ({} variables, {} clauses)",cnf.variables,cnf.clauses.len());
    } else {
        io::stdout().write_all(s.as_bytes()).unwrap_or_else(|e| fail(e));
    }
}
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("solve") if args.len() >= 2 => solve_one(&args[1], &args[2..]),
        Some("bench") if args.len() >= 2 => bench(&args[1], &args[2..]),
        Some("generate") if args.len() >= 2 => generate(&args[1], &args[2..]),
        Some("--help" | "-h" | "help") => print!("{}",usage()),
        Some(file) if !file.starts_with('-') => solve_one(file, &args[1..]),
        _ => { eprint!("{}",usage()); process::exit(2); }
    }
}

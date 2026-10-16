use std::{fs, process::Command};

fn bin() -> &'static str { env!("CARGO_BIN_EXE_cdcl-rust") }

#[test]
fn cli_sat_and_unsat_exit_codes() {
    let sat = Command::new(bin()).args(["solve", "benchmarks/sat_simple.cnf", "--check-model"])
        .output().expect("run SAT");
    assert_eq!(sat.status.code(), Some(10));
    assert!(String::from_utf8_lossy(&sat.stdout).contains("s SATISFIABLE"));
    let unsat = Command::new(bin()).args(["solve", "benchmarks/unsat_xor.cnf"])
        .output().expect("run UNSAT");
    assert_eq!(unsat.status.code(), Some(20));
    assert!(String::from_utf8_lossy(&unsat.stdout).contains("s UNSATISFIABLE"));
}

#[test]
fn cli_tmax_unknown() {
    let o = Command::new(bin()).args(["solve", "benchmarks/pigeonhole_4.cnf", "--tmax", "0"])
        .output().expect("run timeout");
    assert_eq!(o.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&o.stdout).contains("s UNKNOWN"));
}

#[test]
fn cli_generate_and_parse() {
    let file = std::env::temp_dir().join(format!("cdcl-rust-test-{}.cnf", std::process::id()));
    let out = Command::new(bin())
        .args(["generate", "pigeonhole", "--n", "2", "--output", file.to_str().unwrap()])
        .output().expect("generate cnf");
    assert!(out.status.success());
    let input = fs::read_to_string(&file).unwrap();
    assert!(input.contains("p cnf 6"));
    let solved = Command::new(bin()).args(["solve", file.to_str().unwrap()])
        .output().expect("solve generated cnf");
    assert_eq!(solved.status.code(), Some(20));
    let _ = fs::remove_file(file);
}

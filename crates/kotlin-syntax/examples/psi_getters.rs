//! `cargo run --release -p kotlin-syntax --example psi_getters -- --list LIST --root ROOT --out OUT` writes
//! `OUT/<path>.getters.txt` per listed file: what `kotlin_syntax::psi` answers for every node, in the lines
//! `tools/kotlin-psi-getters/GetterOracle.java` prints for the same files, so `diff -r` compares them; `--hash FILE`
//! instead of `--out` writes `<path>\t<FNV-1a 64 of the lines, hex>\t<line count>` per file, as the oracle's `--hash`.

#[path = "../tests/common/getters.rs"]
mod getters;

use std::path::PathBuf;

use kotlin_syntax::{FileKind, Parser};

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut list, mut root, mut out, mut hash) = (None, PathBuf::from("."), None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--list" => list = args.next().map(PathBuf::from),
            "--root" => root = args.next().map(PathBuf::from).unwrap_or(root),
            "--out" => out = args.next().map(PathBuf::from),
            "--hash" => hash = args.next().map(PathBuf::from),
            _ => {
                eprintln!("unknown argument {a}");
                std::process::exit(2);
            }
        }
    }
    let Some(list) = list.filter(|_| out.is_some() || hash.is_some()) else {
        eprintln!("usage: psi_getters --list LIST [--root ROOT] (--out OUT | --hash FILE)");
        std::process::exit(2);
    };
    let mut hashes = String::new();
    let entries = std::fs::read_to_string(&list).expect("read the list");
    let mut parser = Parser::new();
    for entry in entries.lines().filter(|l| !l.trim().is_empty()) {
        let entry = entry.split('\t').next().unwrap_or(entry);
        let source = root.join(entry);
        let Ok(bytes) = std::fs::read(&source) else {
            eprintln!("cannot read {}", source.display());
            continue;
        };
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tree = parser.parse(getters::oracle_text(&bytes), FileKind::from_file_name(&name));
        let lines = getters::lines(&tree);
        if let Some(out) = &out {
            let target = out.join(format!("{entry}.getters.txt"));
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir).expect("create the output directory");
            }
            std::fs::write(&target, &lines).expect("write the lines");
        } else {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &b in lines.as_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            let count = lines.bytes().filter(|&b| b == b'\n').count();
            hashes.push_str(&format!("{entry}\t{h:016x}\t{count}\n"));
        }
    }
    if let Some(hash) = &hash {
        std::fs::write(hash, hashes).expect("write the hashes");
    }
}

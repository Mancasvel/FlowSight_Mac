//! Genera un catálogo numerado de nombres de apps para investigación / referencia.
//!
//! Uso (desde la raíz del repo):
//!   cargo run --manifest-path tools/popular-apps-catalog/Cargo.toml
//!
//! Salida por defecto: `data/popular-apps-catalog.txt`
//! Fuentes: `tools/popular-apps-catalog/assets/business-of-apps-most-popular-2026.txt`
//!          `tools/popular-apps-catalog/assets/supplement.txt`

use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const PUBLISHERS: &[&str] = &[
    "Google",
    "Meta",
    "ByteDance",
    "AZUR Games",
    "Miniclip.com",
    "OpenAI",
    "BabyBus",
    "VOODOO",
    "SayGames",
    "Tencent",
];

const HEADER_TOKENS: &[&str] = &[
    "App",
    "---",
    "Publishers",
    "Users (mm)",
    "Downloads (mm)",
];

fn skip_header_name(name: &str) -> bool {
    name.is_empty() || HEADER_TOKENS.contains(&name) || PUBLISHERS.contains(&name)
}

fn numeric_column_ok(num: &str) -> bool {
    let digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    !digits.is_empty()
}

fn parse_boa_tables(text: &str, re: &Regex) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut skipping_publishers = false;

    for line in text.lines() {
        if line.contains("## Most Popular App Publishers") {
            skipping_publishers = true;
            continue;
        }
        if skipping_publishers {
            if line.starts_with("## ") {
                skipping_publishers = false;
            }
            continue;
        }

        if line.contains("| App |") {
            continue;
        }

        let Some(caps) = re.captures(line.trim()) else {
            continue;
        };
        let name = caps.get(1).unwrap().as_str().trim();
        let num = caps.get(2).unwrap().as_str().trim();

        if skip_header_name(name) {
            continue;
        }
        if !numeric_column_ok(num) {
            continue;
        }
        if name.len() < 2 {
            continue;
        }

        out.insert(name.to_string());
    }

    out
}

fn parse_supplement(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in text.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        if s.len() < 2 {
            continue;
        }
        out.insert(s.to_string());
    }
    out
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("popular-apps-catalog debe vivir en tools/popular-apps-catalog/")
        .to_path_buf();

    let boa_path = manifest_dir.join("assets/business-of-apps-most-popular-2026.txt");
    let sup_path = manifest_dir.join("assets/supplement.txt");

    let mut args = std::env::args().skip(1);
    let out_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("data/popular-apps-catalog.txt"));

    let row_re = Regex::new(r"^\|\s*(.+?)\s*\|\s*([\d.,]+)\s*\|")
        .expect("row regex");

    let mut all: HashSet<String> = HashSet::new();

    match fs::read_to_string(&boa_path) {
        Ok(text) => {
            let boa = parse_boa_tables(&text, &row_re);
            let n = boa.len();
            all.extend(boa);
            eprintln!(
                "[popular-apps-catalog] BOA: {} entradas únicas desde {}",
                n,
                boa_path.display()
            );
        }
        Err(e) => eprintln!(
            "[popular-apps-catalog] Aviso: no se leyó BOA ({}): {}",
            boa_path.display(),
            e
        ),
    }

    let supplement = fs::read_to_string(&sup_path).unwrap_or_else(|e| {
        eprintln!(
            "[popular-apps-catalog] Error leyendo suplemento {}: {}",
            sup_path.display(),
            e
        );
        std::process::exit(1);
    });

    let sup = parse_supplement(&supplement);
    eprintln!(
        "[popular-apps-catalog] Suplemento: {} líneas únicas",
        sup.len()
    );
    all.extend(sup);

    let mut sorted: Vec<String> = all.into_iter().collect();
    sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    if let Some(parent) = out_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let body: String = sorted
        .iter()
        .enumerate()
        .map(|(i, name)| format!("{}. {}", i + 1, name))
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&out_path, body).unwrap_or_else(|e| {
        eprintln!(
            "[popular-apps-catalog] No se pudo escribir {}: {}",
            out_path.display(),
            e
        );
        std::process::exit(1);
    });

    eprintln!(
        "[popular-apps-catalog] Escritas {} apps en {}",
        sorted.len(),
        out_path.display()
    );
}

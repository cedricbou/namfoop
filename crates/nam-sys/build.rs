//! Compile NeuralAmpModelerCore (submodule `vendor/NeuralAmpModelerCore`) + le shim C.
//! Toujours en -O3, même en profil debug : sinon les modèles ne tiennent pas le temps réel
//! et les tests sont inutilisablement lents.
//!
//! Variables d'environnement :
//!   NAMFOOP_NATIVE=0  désactive -march=native / -mcpu=native (binaire portable)

use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let core = root.join("vendor/NeuralAmpModelerCore");
    if !core.join("NAM/dsp.h").exists() {
        panic!("NeuralAmpModelerCore absent : lancer `git submodule update --init --recursive`");
    }

    let mut files: Vec<PathBuf> = Vec::new();
    collect_cpp(&core.join("NAM"), &mut files, 2);
    files.push(root.join("shim/nam_shim.cpp"));

    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let native = std::env::var("NAMFOOP_NATIVE").map(|v| v != "0").unwrap_or(true);
    let cross = std::env::var("HOST").ok() != std::env::var("TARGET").ok();

    let mut b = cc::Build::new();
    b.cpp(true)
        .std("c++20")
        .opt_level(3)
        .define("NAM_ENABLE_A2_FAST", None)
        .define("NDEBUG", None)
        .include(&core)
        .include(core.join("NAM"))
        .include(core.join("Dependencies/eigen"))
        .include(core.join("Dependencies/nlohmann"))
        .flag_if_supported("-ffast-math")
        .flag_if_supported("-fno-finite-math-only") // garde isnan/isinf fiables
        .warnings(false);
    match (target_arch.as_str(), native && !cross) {
        ("x86_64", true) => {
            b.flag_if_supported("-march=native");
        }
        ("aarch64", true) => {
            b.flag_if_supported("-mcpu=native");
        }
        ("aarch64", false) => {
            b.flag_if_supported("-mcpu=cortex-a76"); // Raspberry Pi 5
        }
        _ => {}
    }
    // les architectures s'enregistrent par initialiseurs statiques : garder tous les objets
    b.link_lib_modifier("+whole-archive");
    b.files(&files).compile("nam");

    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=vendor/NeuralAmpModelerCore/NAM");
    println!("cargo:rerun-if-env-changed=NAMFOOP_NATIVE");
}

fn collect_cpp(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() && depth > 1 {
            collect_cpp(&p, out, depth - 1);
        } else if p.extension().is_some_and(|e| e == "cpp") {
            out.push(p);
        }
    }
}

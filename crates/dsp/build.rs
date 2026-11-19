//! Expose le niveau d'optimisation de la compilation aux tests : `crates/dsp/tests/budget.rs` ne
//! peut conclure que sur un binaire optimisé, et `cfg!(debug_assertions)` ne le dit pas — le profil
//! `ci` du projet est optimisé tout en gardant les assertions.
fn main() {
    let opt = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "0".into());
    println!("cargo::rustc-env=NAMFOOP_OPT_LEVEL={opt}");
}

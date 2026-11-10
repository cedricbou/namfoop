//! `namfoop fx` : catalogue des effets et paramètres de chacun (depuis `dsp::fx`).

use anyhow::Result;
use dsp::fx::{catalog, FxSpec};

pub fn run(name: Option<&str>) -> Result<()> {
    let Some(name) = name else {
        println!("Effets (avec --pre / --post, ex: --post gain:db=-6) :");
        for f in catalog() {
            println!("  {:<10} {}", f.kind(), f.help());
        }
        println!("\nParamètres d'un effet : namfoop fx <nom>");
        return Ok(());
    };
    let f = FxSpec::default_of(name)?;
    println!("{} : {}\n", f.kind(), f.help());
    println!("  {:<12} {:>9} {:>9} {:>9} {:>6}  aide", "paramètre", "min", "max", "défaut", "unité");
    for p in f.params() {
        println!(
            "  {:<12} {:>9} {:>9} {:>9} {:>6}  {}",
            p.name,
            p.min,
            p.max,
            p.default,
            p.unit.symbol(),
            p.help
        );
    }
    let example: Vec<String> =
        f.params().iter().map(|p| format!("{}=<{}>", p.name, p.unit.symbol())).collect();
    let example =
        if example.is_empty() { f.kind().to_string() } else { format!("{}:{}", f.kind(), example.join(",")) };
    println!("\nExemple : --post {example}");
    Ok(())
}

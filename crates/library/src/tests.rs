//! Tests de l'index de la bibliothèque : identifiants courts, recherche, packs, presets.
//! `fake_library` sert aussi aux tests des autres modules de la crate — **jamais** `library/` de
//! l'utilisateur.

use super::*;

/// Bibliothèque factice : (chemin relatif, contenu).
pub fn fake_library(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    // dossier unique par appel : les tests tournent en parallèle
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("namfoop-lib-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for (rel, content) in files {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    d
}

fn svt() -> PathBuf {
    fake_library(
        "svt",
        &[
            ("nam/Ampeg SVT/Ampeg SVT - Ultra Lo SM57.nam", "{}"),
            ("nam/Ampeg SVT/Ampeg SVT - Ultra Hi SM57.nam", "{}"),
            ("nam/Ampeg SVT/Ampeg SVT - Gain 10 Ultra Lo and Hi SM57.nam", "{}"),
            ("nam/Ampeg SVT/Ampeg SVT - MD 421.nam", "{}"),
            ("nam/Laney/GP12SMX Angel.nam", "{}"),
            ("nam/Laney/GP12SMX Angel_v2.nam", "{}"),
            ("nam/Laney v2 pack/Scooped.nam", "{}"),
        ],
    )
}

fn name(lib: &Library, q: &str) -> Result<String> {
    lib.resolve(q, Kind::Model).map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
}

#[test]
fn natural_order() {
    let mut v = vec!["Mix 10", "Mix 2", "mix 1", "Mix 11", "Cab10_Mix1", "Cab1_Mix3", "Cab1_Mix1"];
    v.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(v, ["Cab1_Mix1", "Cab1_Mix3", "Cab10_Mix1", "mix 1", "Mix 2", "Mix 10", "Mix 11"]);
}

#[test]
fn keyword_resolution() {
    let d = svt();
    let lib = Library::scan(&d);
    assert_eq!(name(&lib, "md 421").unwrap(), "Ampeg SVT - MD 421");
    // deux fichiers contiennent tous les mots, un seul contient l'expression exacte
    assert_eq!(name(&lib, "ultra lo sm57").unwrap(), "Ampeg SVT - Ultra Lo SM57");
    assert_eq!(name(&lib, "ULTRA LO SM57").unwrap(), "Ampeg SVT - Ultra Lo SM57");
    assert!(name(&lib, "sm57").is_err(), "ambigu");
    assert!(name(&lib, "b7k").is_err(), "absent");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn whole_words_beat_substrings() {
    let d = svt();
    let lib = Library::scan(&d);
    // « v2 » est un mot entier du nom de fichier Angel_v2, pas d'Angel (ni du pack « v2 pack »)
    assert_eq!(name(&lib, "angel v2").unwrap(), "GP12SMX Angel_v2");
    // sans « v2 » : le nom sans mot superflu l'emporte
    assert_eq!(name(&lib, "angel").unwrap(), "GP12SMX Angel");
    assert_eq!(name(&lib, "laney angel").unwrap(), "GP12SMX Angel");
    let err = name(&lib, "sm57").unwrap_err().to_string();
    assert!(err.contains(" m"), "les candidats affichent leur identifiant : {err}");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn ids_are_stable_unique_and_resolvable() {
    let d = svt();
    let a = Library::scan(&d);
    let ids: Vec<&str> = a.entries.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "identifiants uniques");
    assert!(ids.iter().all(|i| i.starts_with('m') && i.len() >= 4));
    // ajouter un fichier ne change pas les identifiants existants (hors collision)
    std::fs::write(d.join("nam/Laney/Other.nam"), "{}").unwrap();
    let b = Library::scan(&d);
    for e in &a.entries {
        let same = b.entries.iter().find(|x| x.rel == e.rel).unwrap();
        assert_eq!(same.id, e.id);
    }
    let target = &a.entries[2];
    assert_eq!(a.resolve(&target.id.to_uppercase(), Kind::Model).unwrap(), target.path);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn cabs_are_indexed_and_resolved_with_irs() {
    let d = fake_library(
        "cab",
        &[
            ("ir/Fender/121 Lower - Cap.wav", ""),
            (
                "cabs/Fender 70 mix.cab.toml",
                "name = \"x\"\n[[ir]]\nfile = \"ir/Fender/121 Lower - Cap.wav\"\n",
            ),
            ("cabs/notes.toml", ""),
        ],
    );
    let lib = Library::scan(&d);
    let cabs: Vec<&Entry> = lib.of_kind(Kind::Cab).collect();
    assert_eq!(cabs.len(), 1, "seuls les .cab.toml");
    assert_eq!(cabs[0].stem, "Fender 70 mix");
    assert!(cabs[0].id.starts_with('c'));
    assert_eq!(lib.resolve_ir("70 mix").unwrap(), cabs[0].path);
    assert_eq!(lib.resolve_ir(&cabs[0].id).unwrap(), cabs[0].path);
    assert!(lib.resolve_ir("121 lower").unwrap().ends_with("121 Lower - Cap.wav"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn colliding_ids_are_lengthened() {
    let mut batch: Vec<Entry> = (0..3000)
        .map(|i| Entry {
            kind: Kind::Ir,
            id: String::new(),
            path: PathBuf::new(),
            rel: format!("ir/p/{i}.wav"),
            pack: "p".into(),
            stem: i.to_string(),
        })
        .collect();
    assign_ids(&mut batch);
    let mut ids: Vec<&str> = batch.iter().map(|e| e.id.as_str()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "3000 IR : identifiants uniques malgré les collisions à 3 caractères");
}

/// Un preset de `presets/` est indexé comme les autres : identifiant court `p…`, nom lisible,
/// trouvable par identifiant et par mots-clés, et relu prêt à jouer (#157).
#[test]
fn presets_are_indexed_searchable_and_loadable() {
    let toml = concat!(
        "name = \"Chill verse\"\n",
        "[[block]]\nkind = \"nam\"\nfile = \"nam/Ampeg SVT/Ampeg SVT - Ultra Lo SM57.nam\"\nid = \"m1\"\n",
        "[[block]]\nkind = \"fx\"\n[block.fx]\ntype = \"reverb\"\n",
    );
    let d = fake_library(
        "presets",
        &[
            ("nam/Ampeg SVT/Ampeg SVT - Ultra Lo SM57.nam", "{}"),
            ("presets/Chill verse.preset.toml", toml),
            ("presets/Punk chorus.preset.toml", "[[block]]\nkind = \"fx\"\n[block.fx]\ntype = \"chorus\"\n"),
            // un mélange d'IR est un TOML aussi : il ne doit pas finir dans les presets
            ("cabs/mix 1.cab.toml", "name = \"mix 1\"\n[[ir]]\nfile = \"x.wav\"\n"),
        ],
    );
    let lib = Library::scan(&d);
    let presets: Vec<&Entry> = lib.of_kind(Kind::Preset).collect();
    assert_eq!(presets.len(), 2, "{presets:?}");
    assert!(presets.iter().all(|e| e.id.starts_with('p')), "identifiants p… : {presets:?}");
    assert!(presets.iter().any(|e| e.stem == "Chill verse"), "nom sans .preset.toml : {presets:?}");
    assert_eq!(lib.of_kind(Kind::Cab).count(), 1, "le mélange reste un mélange");

    // résolution : par mots-clés, puis par identifiant
    let p = lib.resolve_preset("chill").expect("par mots-clés");
    assert!(p.ends_with("presets/Chill verse.preset.toml"), "{p:?}");
    let id = &presets.iter().find(|e| e.stem == "Chill verse").expect("là").id;
    assert_eq!(lib.resolve_preset(id).expect("par identifiant"), p);
    assert!(lib.resolve_preset("inconnu").is_err());

    // relu prêt à jouer : les chemins relatifs sont résolus dans la bibliothèque
    let spec = lib.preset(&p).expect("lecture");
    assert_eq!(spec.name, "Chill verse");
    let blocks = spec.to_blocks(1.0, &|_| 0.0, &|id| lib.path_of_id(id)).expect("les deux blocs");
    assert_eq!(blocks.len(), 2);
    assert!(matches!(&blocks[0], dsp::block::BlockSpec::Nam { path, .. } if path.exists()));

    // où enregistrer : `presets/<nom>.preset.toml`, les séparateurs remplacés
    assert_eq!(lib.preset_path("Chill/verse"), d.join("presets/Chill-verse.preset.toml"));
}

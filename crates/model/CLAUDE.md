# model — types sérialisables

**Responsabilité.** Les types que plusieurs crates lisent : config de l'instrument
(`config.rs` : `Config`, `AudioConfig`, `BackendKind`, `LevelMode`, `LoadedConfig`, chemin par
défaut `~/.config/namfoop/config.toml` ou `$XDG_CONFIG_HOME` ; `data_dir()` : `~/.local/share/namfoop` ou
`$XDG_DATA_HOME`, où vit la bibliothèque par défaut), routage (`routing.rs` : `Routing`,
`OutputRoute`, `Source`), profils de carte (`card.rs` : `CardProfile`, `MixerSetting`, profils livrés
`profiles/*.toml` du dépôt inclus par `include_str!`, `all_profiles`, `find`, `user_dir`).
Les presets ne sont **pas** ici : ce sont des fichiers de la bibliothèque, lus par `library`
(`docs/architecture.md`).

**Pas de logique ici.** Des types, leurs valeurs par défaut, leur nom texte (`Source::name`,
`label`, `FromStr`) et la lecture / écriture TOML. Tout calcul (repères de calibration, routeur
temps réel, conversion vers `audio_io::Kind`) vit dans la crate qui s'en sert. Erreurs :
`thiserror` (`error.rs`), messages vus par l'utilisateur.

**Interdit** (`scripts/check-arch.sh`) : toute dépendance hors `serde`, `toml`, `thiserror` ;
en particulier aucune crate du projet, ni `clap` (les enums CLI comme `BackendArg`, `LevelArg`
sont dans le binaire et se convertissent).

**Compatibilité ascendante des fichiers.** Une config écrite par une version précédente doit
toujours se relire : nouveaux champs avec `#[serde(default)]` (et `skip_serializing_if` pour les
optionnels), jamais de renommage ni de changement de type sans alias serde.
`tests/config-full.toml` et `config-default.toml` sont les **témoins de format** : `tests/files.rs`
les relit et les réécrit à l'octet près, et un échec veut dire que le format a changé.

**Tester** : `cargo test -p model`.

La place de cette crate dans le graphe des dépendances : la carte du `CLAUDE.md` racine et
[`docs/architecture.md`](../../docs/architecture.md).

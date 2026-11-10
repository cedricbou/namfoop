# model — types sérialisables

**Responsabilité.** Les types que plusieurs crates lisent : config de l'instrument
(`config.rs` : `Config`, `AudioConfig`, `BackendKind`, `LevelMode`, `LoadedConfig`, chemin par
défaut `~/.config/namfoop/config.toml` ou `$XDG_CONFIG_HOME`), routage (`routing.rs` : `Routing`,
`OutputRoute`, `Source`), profils de carte (`card.rs` : `CardProfile`, `MixerSetting`, profils livrés
`profiles/*.toml` du dépôt inclus par `include_str!`, `all_profiles`, `find`, `user_dir`).
Plus tard : presets (#6).

**Pas de logique ici.** Des types, leurs valeurs par défaut, leur nom texte (`Source::name`,
`label`, `FromStr`) et la lecture / écriture TOML. Tout calcul (repères de calibration, routeur
temps réel, conversion vers `audio_io::Kind`) vit dans la crate qui s'en sert. Erreurs :
`thiserror` (`error.rs`), messages vus par l'utilisateur.

**Interdit** (`scripts/check-arch.sh`) : toute dépendance hors `serde`, `toml`, `thiserror` ;
en particulier aucune crate du projet, ni `clap` (les enums CLI comme `BackendArg`, `LevelArg`
sont dans le binaire et se convertissent).

**Compatibilité ascendante des fichiers.** Une config écrite par une version précédente doit
toujours se relire : nouveaux champs avec `#[serde(default)]` (et `skip_serializing_if` pour les
optionnels), jamais de renommage ni de changement de type sans alias serde. `tests/files.rs` relit
`tests/config-full.toml` et `config-default.toml` (écrits par l'ancien code) et les réécrit à
l'octet près : un échec veut dire que le format a changé.

**Tester** : `cargo test -p model`.

## Carte détaillée (reprise du `CLAUDE.md` racine)

```
crates/model/     types sérialisables, aucune logique (voir crates/model/CLAUDE.md) : config.rs
                  (~/.config/namfoop/config.toml : trim_db, level, target_db, limiter, calibration_wav,
                  [audio] backend / device / buffer..., [routing]), routing.rs (Routing, OutputRoute,
                  Source) ; tests/ : fichiers de config versionnés relus à l'octet près
```

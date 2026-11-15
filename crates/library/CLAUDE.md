# library — bibliothèque de sons

**Responsabilité.** Tout ce qui concerne les fichiers de sons, sans jamais toucher la carte son :
index de `library/` (identifiants courts `m…` modèles, `i…` IR, `c…` mélanges, `p…` **presets**
(`presets/*.preset.toml`, #157), `s…` **sets** (`sets/*.set.toml`, #217), recherche, métadonnées
.nam), les **sets de concert** (`set.rs`), import des packs
(`import/`), corrections de l'utilisateur (`overrides.rs`, `library/overrides.toml`, par
empreinte du contenu), besoin d'IR et IR conseillées (`recommend.rs`), profils des IR
(`profile.rs`) et des modèles avec la basse de l'utilisateur (`model_profile.rs`).

**Ce qui reste dans le binaire** : options clap et affichage des commandes `import`
(`import_cmd.rs`, bilan), `meta` (`meta_cmd.rs`) et `models` (`models_cmd.rs`). Les structures
`import::ImportArgs` et `overrides::MetaArgs` sont des options simples ; le binaire a leurs
jumelles clap.

**Interdit** (`scripts/check-arch.sh`) : `audio-io`, `engine`, `clap`, `ratatui`. Dépendances :
`dsp`, `nam-sys`, `model`, `serde`, `serde_json`, `toml`, `zip`, `log`.

**Erreurs** (`error.rs`) : type local (pas d'`anyhow` dans une bibliothèque) qui en garde le
comportement : `{:#}` affiche « message : cause : … » (les refus de l'import l'utilisent),
`source()` donne la chaîne au binaire. Messages vus par l'utilisateur : ne pas les changer.

**Tests** : `cargo test -p library`. Toujours une bibliothèque jetable (`tests::fake_library`),
jamais `library/` de l'utilisateur. Les modèles de démo de NeuralAmpModelerCore servent quand un
vrai .nam est nécessaire.

**Taille** : aucun fichier au-delà de 600 lignes (`import/` découpé par responsabilité).

## `set.rs` — les sets de concert (#217)

Le format du § 3 de `docs/design/set.md` : `SetSpec` › `Song` › `Sound`, serde, aller-retour sans
perte (l'édition de l'US 4 en dépend : un défaut ne s'écrit pas, donc le fichier reste lisible à la
main). Un set **référence** des presets par leur identifiant, il ne les copie pas.

Deux sortes de contrôles, et **la différence est une décision de scène** :
- **erreur** (`SetSpec::load` refuse) : `pc` en double — le message **nomme les deux morceaux** —,
  `pc` hors 1-128, `tempo_bpm` hors `dsp::tempo::MIN_BPM`..`MAX_BPM`, mesure illisible. Ce sont des
  fautes qui rendraient la navigation ou la battue imprévisibles ;
- **avertissement** (`SetSpec::warnings`, résolu contre la bibliothèque par `Library::set_warnings`) :
  preset introuvable, prompteur absent. **Le set s'ouvre et le reste joue** : un concert ne s'arrête
  pas parce qu'un son a été renommé.

Permis : un morceau sans son (on écrit un set par étapes) et un morceau sans `pc` (il ne se joue
qu'au pied). `time` est gardé **tel qu'écrit** (`Option<String>`) pour que l'aller-retour ne touche
pas au fichier ; `Song::time_sig()` le lit (`TimeSig`, déjà vérifié au chargement).

## Faits mesurés

- Bibliothèque : 55 modèles, tous au format NAM 0.7.0 `SlimmableContainer` (WaveNet 3ch + 8ch,
  `--size 0` = 3ch), loudness entre -14 et -40 dB (d'où la normalisation à -18 dB). 144 IR, surtout
  48 kHz mono, quelques-unes en 44,1 / 96 kHz (rééchantillonnées au chargement), jusqu'à 6 s (tronquées
  à 200 ms).
- Métadonnées des packs achetés (komposition101, 84 modèles) : `gear_type = amp` partout, FullRig
  compris ; marque / modèle `T3K-Null`, `tz-make` ; pas d'`input_level_dbu` ; loudness -9 à -13 dB.
  Seul le nom (`_DI`, `_FullRig`) ou le dossier (`Full-Rig Profiles/`) est fiable. Les mêmes IR sont
  livrées dans plusieurs packs (25 copies exactes sur 266, dont une au sein d'un même pack :
  MBP2 Cab8_Mix2 = Cab10_Mix3). Formats NAM 0.7.0 A2 SlimmableContainer et
  0.5.4 WaveNet, IR float 32 bits mono/stéréo : tout se charge.
- Deviner « baffle inclus » par le son NE MARCHE PAS en basse : pente aigus (6–10 kHz) / médium
  (300–1500 Hz) sur bruit blanc, de -30 à -1 dB pour les FullRig comme pour les DI, et même au sein
  d'une paire DI/FullRig du même son l'écart va de -7 à +10 dB (baffles basse avec tweeter). Abandonné.
- Profil des IR : les 11 « Mix » AMPG 410 de K101 diffèrent de ±2 à ±5 dB par zone (sub, présence,
  air) une fois comparés à la médiane du pack ; une étendue aiguë « à -6 dB » est trompée par le
  creux vers 1 kHz des baffles basse, d'où le seuil -12 dB mesuré depuis les extrémités.
- Grain des modèles : la THD d'un sinus 110 Hz à la crête calibrée suit le drive de la Rat (0,2 % à
  1,5 ; 6 % à 6 ; 18 % à 7 ; 28–38 % à 9–10) et le Bassman 800 « presque clair » (0,9 %). La
  cohérence entrée/sortie sur le jeu réel a été essayée et abandonnée : 55 % pour tous les
  FullRig / captures micro (réponse longue), 4 % pour une Rat à 1,5. Un mi grave pur sous-estime
  (beaucoup d'amplis filtrent le grave avant le gain). Coût : 84 modèles komposition en 12 s sur
  16 threads (A2 ≈ ×30 temps réel, WaveNet 16ch ≈ ×13) ; 55 modèles Tone3000 en 3,4 s.

## Carte détaillée (reprise du `CLAUDE.md` racine)

```
crates/library/   bibliothèque de sons, sans carte son (voir crates/library/CLAUDE.md) :
                  lib.rs (index de library/, identifiants m…/i…/c…/p… (presets)/s… (sets), recherche,
                  métadonnées .nam ; resolve_preset / preset / path_of_id / preset_path,
                  resolve_set / set / set_warnings / set_path),
                  set.rs (sets de concert : SetSpec/Song/Sound, TimeSig, erreurs vs avertissements),
                  import/ (mod.rs : run -> Outcome ; scan.rs, classify.rs, validate.rs),
                  overrides.rs, recommend.rs, profile.rs, model_profile.rs, error.rs
```

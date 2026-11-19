# library — bibliothèque de sons

**Responsabilité.** Tout ce qui concerne les fichiers de sons, sans jamais toucher la carte son :
index de `library/` (identifiants courts `m…` modèles, `i…` IR, `c…` mélanges, `p…` **presets**
(`presets/*.preset.toml`), `s…` **sets** (`sets/*.set.toml`), recherche, métadonnées
.nam), les **sets de concert** (`set.rs`), le **prompteur ChordPro**
(`prompt.rs` : lecture pure du sous-ensemble du § 10 de `docs/design/set.md` — sections,
accords en ligne, cues, les quatre directives `x_namfoop_…` ; **tout le reste est ignoré sans
erreur**, parce qu'un fichier écrit pour un autre logiciel doit s'afficher ici, et seule
l'ouverture du fichier peut échouer. Ni écran ni tempo : le placement dans le temps est la
battue, l'affichage `namfoop`. namfoop n'écrit JAMAIS un `.cho`, sauf le squelette de `t`
— `Prompt::skeleton` —, parce que ces fichiers viennent d'ailleurs), import des packs
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

## `set.rs` — les sets de concert

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
- Deviner « baffle inclus » par le son NE MARCHE PAS en basse, d'où le recours au nom et au dossier :
  la pente aigus (6–10 kHz) / médium (300–1500 Hz) sur bruit blanc va de -30 à -1 dB pour les
  FullRig comme pour les DI, et même au sein d'une paire DI/FullRig du même son l'écart va de -7 à
  +10 dB (baffles basse avec tweeter).
- Profil des IR : les 11 « Mix » AMPG 410 de K101 diffèrent de ±2 à ±5 dB par zone (sub, présence,
  air) une fois comparés à la médiane du pack ; une étendue aiguë « à -6 dB » est trompée par le
  creux vers 1 kHz des baffles basse, d'où le seuil -12 dB mesuré depuis les extrémités.
- Grain des modèles : la THD d'un sinus 110 Hz à la crête calibrée suit le drive de la Rat (0,2 % à
  1,5 ; 6 % à 6 ; 18 % à 7 ; 28–38 % à 9–10) et le Bassman 800 « presque clair » (0,9 %). La
  cohérence entrée/sortie sur le jeu réel, elle, ne discrimine pas : 55 % pour tous les
  FullRig / captures micro (réponse longue), 4 % pour une Rat à 1,5. Un mi grave pur sous-estime
  (beaucoup d'amplis filtrent le grave avant le gain). Coût : 84 modèles komposition en 12 s sur
  16 threads (A2 ≈ ×30 temps réel, WaveNet 16ch ≈ ×13) ; 55 modèles Tone3000 en 3,4 s.

La place de cette crate dans le graphe des dépendances : la carte du `CLAUDE.md` racine et
[`docs/architecture.md`](../../docs/architecture.md).

# library — bibliothèque de sons

**Responsabilité.** Tout ce qui concerne les fichiers de sons, sans jamais toucher la carte son :
index de `library/` (identifiants courts `m…` modèles, `i…` IR, `c…` mélanges, `p…` **presets**
(`presets/*.preset.toml`), `s…` **sets** (`sets/*.set.toml`), recherche, métadonnées
.nam), les **sets de concert** (`set.rs`), la **carte des mesures** (`bars.rs`), le
**prompteur ChordPro**
(`prompt.rs` : lecture pure du sous-ensemble du § 10 de `docs/design/set.md` — sections, leurs
attributs `label="…"` (ChordPro 6), accords en ligne, cues (une cue hors section est celle de la
section qui suit), tonalité, les quatre directives `x_namfoop_…`, et les **grilles** — une ligne de
mesures est lue par `grid.rs`, § 2 de `docs/design/prompteur.md`, signes gardés tels qu'écrits ;
**tout le reste est ignoré sans erreur**, parce qu'un fichier écrit pour un autre logiciel doit s'afficher ici, et seule
l'ouverture du fichier peut échouer. Ni écran ni tempo : le placement dans le temps est la
battue, l'affichage `namfoop`. namfoop n'écrit JAMAIS un `.cho`, sauf le squelette de `t`
— `Prompt::skeleton` —, parce que ces fichiers viennent d'ailleurs), import des packs
(`import/`), corrections de l'utilisateur (`overrides.rs`, `library/overrides.toml`, par
empreinte du contenu), besoin d'IR et IR conseillées (`recommend.rs`), profils des IR
(`profile.rs`) et des modèles avec la basse de l'utilisateur (`model_profile.rs` : le `Signal`
vient du binaire, qui a résolu l'instrument joué), et **mes instruments** (`instrument.rs`,
`library/instruments/<id>.toml`, § 14 de `docs/design/instruments.md` : lecture, contrôles —
numéro en double, deux « par défaut », chacun nommant les deux fichiers —, désignation par
numéro, nom ou mots, écriture sans perte ; le choix de l'instrument joué et de la carte est au
binaire).

## `exchange/` — la valise et la mémoire des échanges

Faire passer la bibliothèque d'une machine à l'autre par une **valise** posée sur une clé USB
(`<clé>/valise-namfoop/` : l'étiquette valise.toml, et `library/`, la bibliothèque telle quelle).
`exchange/mod.rs` : l'**élément** (famille, nom affiché, chemin, empreinte), l'**inventaire** d'une
bibliothèque — chaque fichier empreint (`bytes_hash`), `.cache/` et `.avant-import/` exclus — et la
**différence** entre deux états ; `exchange/label.rs` : l'étiquette, aller-retour sans perte ;
`exchange/memory.rs` :
le nom de la machine et son dernier échange avec chaque valise, dans un dossier que le binaire
choisit ; `exchange/suitcase.rs` : reconnaître une valise, le plan d'un export et son écriture ;
`exchange/import.rs` : la comparaison à trois (ici, valise, dernier échange → arrive, reste, à
trancher, identique), le plan de l'import — une réponse prime sur la comparaison —, les fichiers
abîmés, l'écriture ; `exchange/describe.rs` : le résumé d'une version et ce qui diffère entre deux
(preset, set, calibration, correction, sons), ce que montre la question d'un élément à trancher.

Les contrats que le reste suppose :

- **un fichier est un élément, sauf deux** : `instruments/<id>.toml` donne un élément
  « instrument » (nom, numéro, défaut) et un par calibration (table + enregistrement), clé
  `instruments/<id>.toml#<carte>` ; `library/overrides.toml` un par modèle corrigé, clé « overrides.toml#<empreinte> ».
  L'empreinte d'un sous-élément est celle de sa sérialisation canonique : réécrire le fichier sans
  changer la table ne change rien ;
- **« changé » se décide sur l'empreinte**, jamais sur une date : les dates de l'étiquette
  s'affichent seulement ;
- **l'ordre d'écriture d'un export** (étiquette `complete = false`, fichiers sous nom temporaire
  vidés puis renommés, dossiers vidés, étiquette complète en dernier) est ce qui rend « tu peux
  retirer la clé » vrai ; une valise incomplète se relit fichier par fichier, jamais sur son étiquette ;
- **un nom qu'une clé FAT refuse** est un refus qui le nomme (`portable_names`), jamais un renommage.
- **une valise est en avance** (`suitcase::ahead`) quand un élément y diffère à la fois du dernier
  échange et d'ici : `export` refuse alors d'écrire par-dessus ;
- **la mémoire d'un échange est l'état de la valise** à ce moment-là, après un export comme après un
  import : un élément gardé d'ici reste donc un changement d'ici, et part au prochain export ;
- **un import déplace** ce qu'il remplace ou supprime dans `.avant-import/` (vidé à l'import
  suivant) ; un fichier qui porte plusieurs éléments est recopié tel quel s'ils viennent tous de la
  valise, réécrit fusionné sinon — et une calibration ne reste jamais sans son instrument.

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
perte (l'édition du set en dépend : un défaut ne s'écrit pas, donc le fichier reste lisible à la
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

## `bars.rs` — la carte des mesures

Le § 11 de `docs/design/set.md`, **pur** : `Meter` (une mesure et le regroupement de ses temps),
`Span` (une section : métrique, tempo, durée en mesures, son lié, et pour une grille les mesures de
chaque ligne) et `BarMap`, bâtie d'un `Song` et de son `Prompt`. `place(section, mesure)` donne la
ligne et, dans une grille, la mesure dans la ligne : c'est celle que le prompteur allume. Ni
écran, ni horloge — la position dans le temps est la battue
(`crates/namfoop/src/browse/state/set/beat.rs`).

Cinq contrats que le reste du code suppose :

- le **BPM se compte à la noire**, toujours (Ableton, et les 24 tops par noire de l'horloge MIDI).
  Le **tick** d'une mesure est son unité : la croche en x/8, la noire en x/4, d'où
  `Meter::tick_ms(bpm)` qui prend le tempo en **argument** — le tempo entendu est arbitré ailleurs,
  et la carte ne porte que ce que les fichiers déclarent ;
- le regroupement par défaut est **par trois en x/8 à partir de 6** (6/8, 9/8, 12/8) et **temps par
  temps** partout ailleurs. Un `{x_namfoop_beats}` dont la somme ne fait pas le nombre de temps de
  la mesure est **ignoré** : la mesure fait foi, le regroupement l'orne ;
- le morceau remplace la valeur **initiale** du prompteur, pas ses changements : le premier
  `{tempo}` / `{time}` du `.cho` ne sert que si le `.set.toml` se taît, et une directive plus loin
  dans le morceau gagne — y compris si elle revient à la valeur de départ ;
- la durée d'une section est `{x_namfoop_bars}`, sinon **la somme de sa grille** (`%` compte une
  mesure, les reprises ne se déplient pas), sinon la part de `{duration}` ;
- une section **sans durée** (`bars: None`) ne finit jamais d'elle-même : la battue y compte les
  mesures. C'est aussi le cas d'un morceau sans prompteur, qui a une section implicite — c'est ce
  qui garantit qu'une bande se dessine toujours.

`at_sixteenths` convertit un **Song Position Pointer** de MIDI (des doubles croches depuis le début)
en section / mesure / tick, et `sixteenths_at` fait l'inverse ; les deux rendent `None` dès qu'une
section de durée inconnue est en travers, parce qu'on ne peut alors pas compter depuis le début.

## Faits mesurés

- Inventaire de l'échange (`exchange::inventory`, binaire optimisé, PC) : 628 fichiers, 73 Mo, en
  0,45 s cache disque froid et 0,08 s chaud — aucun cache d'empreintes n'est nécessaire.
- Bibliothèque de l'utilisateur (relevé non refait depuis qu'elle a grandi) : modèles NAM 0.7.0
  `SlimmableContainer` (WaveNet 3ch + 8ch, `--size 0` = 3ch), loudness entre -14 et -40 dB (d'où la
  normalisation à -18 dB) ; IR surtout 48 kHz mono, quelques-unes en 44,1 / 96 kHz (rééchantillonnées
  au chargement), jusqu'à 6 s (tronquées à 200 ms).
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

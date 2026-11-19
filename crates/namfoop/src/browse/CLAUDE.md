# browse — les écrans

**Responsabilité.** L'écran plein écran `browse` : trois espaces (Bibliothèque, Chaîne, Set), la
chaîne éditée en blocs, les presets, le set de concert et le prompteur. `state/` est de la logique
**pure** (horloge fournie, testée sans terminal ni carte), `ui/` ne fait que dessiner (ratatui,
`TestBackend`), et `mod.rs` est le seul à toucher aux fichiers, au terminal et au thread audio.

Spécifications : [`browse-espaces.md`](../../../../docs/design/browse-espaces.md),
[`browse-chaine.md`](../../../../docs/design/browse-chaine.md),
[`set.md`](../../../../docs/design/set.md). Le reste du binaire (CLI, commandes, `setup`) :
[`crates/namfoop/CLAUDE.md`](../../CLAUDE.md).

## Les trois espaces — `state/space/`

`Space::Library` (onglets Modèles / IR / Mélange : on entend le banc d'essai basse › modèle › IR
**sans aucun effet**), `Space::Chain` (on entend la chaîne) et `Space::Set` (la vue scène).
`1` / `2` / `3` changent d'espace et l'écoute suit ; `Tab` et `→` `←` restent dans la Bibliothèque
(`switch_tab`, testée dans les deux sens : `Tab` / `Maj+Tab` bouclent sur les trois onglets, `→` /
`←` vont de proche en proche).

**Les espaces sont indépendants.** `step_preset` (la banque, en boucle) est à la **Chaîne** ;
`step_set` (la liste à plat du set) est à l'**espace Set** et à la scène. Ne jamais détourner l'un
vers l'autre : à la maille preset, on travaille dans la Chaîne. Même règle en sens inverse —
l'espace Set **éclipse** `n`, pour qu'une touche d'un espace n'agisse pas sur l'objet d'un autre.

## Le clavier

### La table — `state/bindings/`

**Ce que chaque contexte possède est déclaré une fois**, ici : `label` et `what` (l'aide), `bar`
(la barre du bas), `keys` (la jointure). `ui/footer.rs` et `ui/windows.rs::help_lines` en
**dérivent** leur texte : ajouter une touche, c'est ajouter une ligne ici.

### Qui a le clavier — `state/context.rs`

`State::context()` nomme le contexte actif. `context_under_help()` existe parce que `h` explique le
contexte **du dessous**.

### L'ordre des couches — `state/keys/mod.rs` (ADR 0009)

`on_key` est un `match` sur `State::context()` : **une ligne par contexte**, qui dit qui le traite.
Le `match` est **exhaustif**, donc un `Context` nouveau sans preneur ne compile pas. Un **mode**
(scène verrouillée, confirmations, aide, éditeur, saisies, fenêtres modales) a sa fonction, qui ne
retombe jamais ; un contexte d'**espace** descend les quatre étages de `tiers` : **la pédale**
(`1 2 3 q h * k`, qu'un espace ne peut **pas** éclipser : un risque live, pas une esthétique) >
**l'espace** (`set_key` / `fx_key` / `library_key`) > **les communes** (`S P n`, éclipsables par un
espace **qui le déclare**) > **le dernier recours** (`Échap`, et la touche sans propriétaire posée
dans `State::unhandled`, que la boucle trace). L'espace passe avant les communes : seule issue
tenable avec 20 lettres sur 26 déjà prises. Deux règles seulement précèdent le contexte : un mur ne
se traverse pas, et `h` ouvre l'aide hors saisie.

Quatre règles à ne pas casser :

- **une modale ne retombe jamais** — sauf devant la **pédale**, qui la **referme** d'abord (comme
  `Échap`, avec l'annulation que la fenêtre connaît) puis agit : `1 2 3 q * k` répondent pareil
  partout, et rester coincé dans une fenêtre est un risque en concert. Une touche qui *traverse* une
  modale, elle, agirait derrière elle ;
- **un panneau non plus.** Les réglages d'un effet (`FxFocus::Params`) gardent une liste
  **blanche** — une liste de refus échoue **ouverte**. Passent ce qui écoute ou montre (`,` `.` `r`
  `v` `S` `P`) et `PgDn` / `PgUp`, le geste du pédalier, qui ne se bloque jamais ;
- **`library_key` rend `None` sur une touche inconnue**, sinon les étages du dessous meurent dans la
  Bibliothèque. Sa couche est la dernière et porte sa **garde d'espace** : sans elle, une touche
  qu'un espace ne consomme pas agit sur un écran qu'on ne regarde pas (depuis la vue scène, `b`
  couperait l'IR du son joué, `e` ouvrirait l'éditeur de modèle). `Échap` et `q` valent partout ;
- **toute saisie de texte** (nom, filtre des listes, filtre du catalogue) passe **avant** les
  touches communes, sinon une lettre commune est volée au filtre et agit deux fois.

### L'oracle — `state/keyboard.rs`

Un test qui mesure, pour **chaque** contexte et **chaque** touche (`all_keys()`), ce qui se passe
vraiment — couche qui consomme, action, message, champs d'état changés — et le fige dans
`keyboard.ref.txt`. **C'est lui qu'on lit pour savoir ce que fait une touche** : `on_key` est une
cascade de fonctions réparties sur neuf fichiers, la lire ne suffit pas.

Régénérer **après avoir voulu le diff** :
`NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop keyboard`.

Deux pièges d'écriture : chaque touche part d'un état **neuf** (sinon on mesure une séquence), et
on ne regarde **jamais** l'écran (ratatui n'écrit que les cellules modifiées).

## La chaîne éditée — `state/chain/`, `state/edit/`, `state/label.rs`

`chain/` porte la chaîne en blocs (`Leaf` / `Split` / `Branch`) et le curseur 2D (`Cursor` /
`Spot`), dont `Cursor::Merge` : la **sortie** d'un parallèle, le ◇ de son bord droit, d'où `a`
ajoute après lui (le bloc Merge / Mixer du marché ; tout le reste du parallèle se règle depuis
le ◆). `edit/` tient la navigation 2D et les règles d'édition (`s`, `+`, `x`, `[ ]`, `{ }`, `b`,
`m` / `o` / `p`) ; `label.rs` les étiquettes du synoptique.

Le **volume** (`,` / `.`, `adjust_volume`) et le **tempo** du preset vivent dans `chain::Chain`,
**avec** les blocs, pour qu'`unsaved()` les voie sans code en plus.

L'affichage d'une valeur de réglage passe par `dsp::fx::Unit::format` (et non `chain::num` +
`symbol()`) : une division se montre par son nom de note (`1/8.`). Un paramètre remplacé par une
division se montre **entre parenthèses** avec la durée que le tempo lui impose (`(375 ms)`,
`(2000 ms, plafond)`), et le régler est refusé **avec la marche à suivre** : ne jamais afficher un
nombre qui n'agit pas.

## Ce qu'on entend — `state/target.rs`, `plan.rs`, `build.rs`, `mod.rs`

`target.rs` dit la chaîne telle qu'on l'écoute **selon l'espace** : banc à nu, ou blocs édités avec
l'essai du sélecteur et l'écoute isolée. `plan.rs` en fait la demande au thread de construction
(chemins résolus, blocs coupés retirés, solo en muets).

`build.rs` est **le thread de construction** : il possède la `engine::bank::Bank` — donc l'envoi de
chaînes —, bâtit la chaîne demandée hors temps réel avec sa table de vumètres
(`dsp::meter::ChainMeters`) et **précharge** les chaînes des presets entre deux demandes, si bien
qu'une bascule n'installe qu'une chaîne déjà prête. Une demande porte la place de banque dont elle
est exactement la chaîne (`Request::preset`, égalité des blocs résolus) ; dès qu'on édite, isole ou
essaie un son, elle vaut `None` et la chaîne est rebâtie, et `Job::Bank(reqs)` rejoue le
préchargement après chaque enregistrement de preset. `drain()` vide la file sans bloquer, **la
dernière chaîne demandée gagne**, et elle est rappelée **juste avant** d'installer : une chaîne
bâtie pendant que l'utilisateur a déjà rappuyé est **jetée** plutôt qu'installée, sinon on entend le
preset d'il y a un ou deux appuis. Le préchauffage (`bank.prewarm`, 26 à 54 ms) précède donc cette
vérification, et l'installation passe par `bank.install_warm`.

**Les paramètres vivants** (ADR 0007, ADR 0008) ne passent pas par là : `mod.rs` tient les cases
`dsp::param::Control` du volume, du tempo et du clic, et les écrit d'après `st.plan()` **juste
après** le `match action` — pour qu'une chaîne installée par ce tour-ci les lise dès son premier
bloc — et les trace quand elles changent (`volume=±x.x`, `tempo=xxx.x`). Le thread de construction
les pose en `Volume::Live` / `Tempo::Live` dans **toutes** les chaînes, banque comprise.

**Le clic du métronome** suit le même chemin (`gain`, à zéro quand c'est éteint, et `downbeat` =
`Arbiter::beat_epoch`). `k` le bascule (`State::metronome`, éteint au démarrage, jamais dans un
preset : `unsaved()` ne le voit pas) et il ne va que dans les sorties déclarées **in-ear** par
`setup` (`OutputRoute::click`, posé par `setup/state.rs` d'après `OutRole::InEar`) : c'est ce qui
garantit qu'il n'atteint jamais le sondier. `State::metronome_routed` dit si une sortie le reçoit ;
sinon `k` renvoie à `setup` au lieu d'allumer un clic inaudible.

## Les presets — `state/preset/`

Conversion chaîne ↔ `dsp::preset::PresetSpec` (index de la bibliothèque ↔ chemins relatifs +
identifiant en filet), la banque `P` (presets déjà convertis : charger ne lit aucun fichier), `S`
(nom proposé, écrit par `mod.rs` via `Action::SavePreset`), l'indicateur de l'en-tête (`unsaved()`,
dont le preset est la référence : `mark_preset_saved`), `t` le TOML replié, la bascule qui demande
une fois (`confirm_switch`) et `n` la **chaîne vierge** (chaîne vide, preset **détaché** pour que
`S` propose un nom neuf, `mark_start` sur la chaîne vide). Les deux passent par
`pending_switch: Option<Pending>` (`Preset(i)` | `New`), que `mod.rs` reprend après l'écriture.

**Deux tempos distincts** : `fx.chain.tempo_bpm` est le tempo **enregistré** (ce que `S` écrit, ce
que `unsaved()` compare ; seuls un tap et un chargement de preset le changent), et
`State::heard_tempo_bpm()` le tempo **entendu**, celui de l'arbitre `engine::tempo::Arbiter` que
`State` possède. C'est l'entendu qui va dans la case et à l'écran, et les deux diffèrent dès qu'une
horloge MIDI tourne. `*` = tap (`State::tap_tempo`), et `tick()` appelle `Arbiter::poll` pour voir
une horloge qui se tait. `*` n'agit que dans l'espace **Chaîne** : le banc d'essai n'est pas un
preset.

## Le set — `state/set/`

`mod.rs` : `ActiveSet` porte le `SetSpec` lu par `library::set` et, pour chaque son, sa place dans
`State::presets` (`None` : preset introuvable, le son est **sauté**). `flat()` est la liste à plat
du § 5 ; `step()` / `step_song()` **ne bouclent pas** (on ne rejoue pas le premier morceau par
mégarde en fin de set) ; `distinct_slots()` donne la banque du set — **une** place par preset
distinct, parce que le volume et le tempo sont vivants, donc hors de l'identité d'une demande de
chaîne : un preset joué dans dix morceaux n'occupe qu'une place et ne rebâtit rien d'un morceau à
l'autre (vérifiable dans la trace : `place=0/1/2`, `préchargée`). `play_set_sound` ajoute le volume
du morceau à celui du preset et passe son tempo à l'arbitre comme le ferait un preset.
`load_preset` ne quitte **pas** l'espace Set : jouer un son du set ne sort pas de l'écran de concert.

`screen.rs` — l'espace Set : le curseur (**viser n'est pas jouer** : `↑↓` ne changent rien à
l'oreille, `Entrée` joue le morceau visé sur son son 1), `i` (plusieurs morceaux peuvent rester
dépliés), **les touches du pied** (`PgDn` / `PgUp` = morceau suivant / précédent, à lier **ici**,
parce que `step_preset` n'est atteint que par `fx_key`, qui ne tourne que dans l'espace Chaîne) et
la fenêtre `L`, qui rend `Action::ListSets` puis `OpenSet` / `SetChanged` — la boucle seule voit la
bibliothèque. Le set actif est retenu dans `[set] default` de la config, que `run --set` sans
argument prend aussi : une seule notion de « set du moment ».

`edit.rs` — l'édition du set (§ 8) : `e` l'ouvre, `S` y **éclipse** le `S` du preset et rend
`Action::SaveSet(stem)` que la boucle écrit. `ActiveSet::saved` est la référence du `●`
(`unsaved()`), `resync()` refait la correspondance son -> place de banque après chaque édition, et
`songs_using(slot)` compte les morceaux qui jouent un preset (le titre de la Chaîne : « preset
utilisé par N morceaux de ce set »). Trois choses à ne pas casser :

1. ses **sous-modes** (saisie, liste des presets, confirmation de `X`) sont appelés à l'étage des
   **modes**, donc avant la pédale — sinon taper `1` dans un titre change d'espace et `q` quitte au
   milieu d'un nom ;
2. la **table** de l'édition reste à l'étage de l'espace et ne laisse **rien** tomber sur les
   touches de la vue scène (sinon `L` y change de set en abandonnant les modifications sans rien
   demander) ; seules `PgDn` / `PgUp` passent, le geste du pied ne se bloque jamais ;
3. `↑↓` dans la liste des presets **écrivent** l'essai dans le son (c'est ce qui le fait entendre),
   donc `Échap` a quelque chose à défaire : `Picking::was` garde le preset d'avant. Le sélecteur de
   la Chaîne, lui, n'a pas ce problème — son essai n'est posé que par `plan()`.

`prompt.rs` — le prompteur (§ 10) : la lecture du ChordPro est dans `library::prompt`, ici il n'y a
que la **position** (`ActiveSet::prompt_section` / `prompt_line`) et ce qui la déplace — le son
(`follow_prompt`, appelé par `play_set_sound` **avant** de bouger `at`, parce que la règle dépend de
si l'on change de morceau ou seulement de son) et `Fin` / `Début` (`step_prompt`, réglables par
`[set] prompt_next` / `prompt_prev`, **permises** sur scène : elles ne changent rien). Les
prompteurs sont dans `State::prompts`, **indexés par chemin** et non par numéro de morceau —
l'édition d'un set déplace les morceaux, une liste parallèle se désalignerait au premier `[`. `t`
rend `Action::EditPrompt` : la boucle seule touche aux fichiers et au terminal.

## Les autres panneaux — `state/`

Chacun porte son contrat dans son en-tête `//!` ; voici la carte, et ce qui ne s'y trouve pas.

- `send/` : de la Bibliothèque vers la chaîne (§ 4) — fenêtre « quel bloc remplacer », case « avec
  son IR », mode pose. `heard_bench` / `chain_text` donnent la ligne d'écoute.
- `picker/` : le sélecteur sur place d'un bloc `nam` / `ir` (§ 5) — **l'essai n'écrit rien**, il
  n'est posé que par `plan()`, donc `unsaved()` l'ignore et `Échap` n'a rien à défaire.
- `fx.rs` : l'onglet Effets — catalogue, réglages de l'élément sous le curseur (effet, parallèle ou
  branche), écoute différée, `v` les vumètres. La fenêtre du catalogue est dessinée **en dernier**
  par `ui/fx.rs`, après le panneau du bas, sinon elle disparaît sur un bloc `nam` / `ir`.
  `State::field_help` explique le réglage sous le curseur : la colonne de valeur ne fait que
  **9 caractères**, donc tout ce qui doit s'expliquer va dans cette ligne.
- `bands.rs` : les 7 zones de `library::profile::ZONES` pour la chaîne **écoutée**, ou l'écart avec
  la référence ◆. `WIDE_COLUMNS` = 128 est la source unique de `SPECTRUM_MIN_WIDTH`, et
  `show_bands()` suit `z`, ou à défaut la largeur de l'écran.
- `meters.rs` : les vumètres du synoptique et `level_align_db`, **l'alignement sur le DI**. Il
  annule tout bloc `gain` : `ui/synoptic.rs` le montre grisé en bout de chaîne, le panneau de
  réglages prévient (`State::gain_cancelled_by_alignment`) et l'ajout d'un `gain` le dit.
- `compare.rs` l'A/B `b` / `r` et la référence `c` ; `editor.rs` l'éditeur `e` (écoute du gain,
  grain remesuré) ; `blend.rs` les mélanges ; `lists.rs` les listes des onglets Modèles / IR.
- `quit.rs` : `unsaved()` (**tout**, y compris le set édité — la question de `q`) est séparée de
  `chain_unsaved()` (**la chaîne seule** — la question d'un remplacement de chaîne : `PgDn`, la
  banque, `n`) ; dans le même sac, chaque essai de preset pendant l'édition d'un set ouvre la
  fenêtre « non enregistré » et rend l'édition impraticable.

## Le dessin — `ui/`

Ratatui, testé sur `TestBackend` : un fichier par zone (`mod.rs` listes et barre d'état,
`synoptic.rs`, `spectrum.rs`, `bands.rs`, `rows.rs`, `picker.rs`, `set.rs`, `editor.rs`, `fx.rs`,
`footer.rs`, `windows.rs`). Ce qui ne se devine pas en les lisant :

- `help_bar` (`mod.rs`) coupe la barre aux séparateurs de touches en gardant **toujours** les trois
  dernières (`h` aide / l'autre espace / `q` quitter) ;
- les **largeurs** commandent les panneaux : le Spectre à partir de 128 colonnes (police 10×20 du
  Pi), les Bandes sous lui au-delà de 128 et sous la liste en dessous (±12 dB sur 5 lignes, 3 dB par
  demi-bloc, ▲/▼ hors échelle), la set list sur 44 colonnes au-delà de 100, 30 en dessous et une
  ligne compacte sous 70 ;
- `footer.rs` ne décide rien : il montre les touches de l'écran où l'on est, la première couche qui
  a la main gagne. `windows.rs` dessine l'**overlay d'aide** `h`, contextuel, par-dessus tout.

## Tester

La logique se teste dans `browse::state`, sans terminal. De bout en bout, `scripts/check-e2e.py`
pilote l'écran dans un pseudo-terminal (`--no-config --backend sim`, bibliothèque **jetable**
`--library <tmp>` : `browse` écrit les mélanges dans `<library>/cabs/`, ne jamais polluer la
bibliothèque de l'utilisateur) ; les pièges du pilotage y sont **implémentés**.

`NAMFOOP_BROWSE_TRACE=<fichier>` écrit chaque chaîne réellement installée dans le thread audio
(modèle, IR, entrée, effets pré / post, taille de la table de vumètres) et chaque grain mesuré par
l'éditeur. **Pour ce qu'on entend, c'est elle qui fait foi** — ratatui n'écrit que les cellules
modifiées, donc chercher une valeur à l'écran retrouve l'ancienne. Chaque ligne est **horodatée**
(ms depuis la première) et écrite en **un seul** appel, sinon les lignes des deux threads se
coupent ; le fichier n'est lu dans l'environnement qu'**une fois**. Une chaîne bâtie puis **jetée**
parce qu'une demande plus récente est arrivée le dit aussi (`JETÉE`).

## Faits mesurés

- **Bascule d'un preset** (`PgDn` / `PgUp` ; x86, profil `ci`, backend sim, mesurée en
  pseudo-terminal : de la touche à la ligne `NAMFOOP_BROWSE_TRACE`, écrite quand la chaîne est
  **dans** le thread audio) : **20 à 21 ms** depuis la banque préchargée, pour les trois presets de
  test — y compris le parallèle à **deux** NAM. C'est le fondu, et c'est aussi le plancher de la
  mesure (la boucle d'affichage scrute le clavier toutes les 30 ms). Un preset pas encore préchargé
  passe par la construction complète, que le **chargement des modèles** domine (≈ 80 ms le premier,
  ≈ 120 ms le second) et non le nombre d'effets ; le son ne se coupe ni dans un cas ni dans
  l'autre, la chaîne précédente joue jusqu'au fondu. **Pi 5 non mesuré** ; coût mémoire et détail du
  préchargement : [`crates/engine/CLAUDE.md`](../../../engine/CLAUDE.md).
- **Le tap (`*`) de bout en bout** (pseudo-terminal, bibliothèque jetable, trace) : un preset à
  `tempo_bpm = 92` écrit `tempo=92.0 (preset)`, deux frappes à 522 ms donnent `tempo=114.9 (tap)`,
  une 3ᵉ une seconde plus tard **repart** une séquence (`59.3`, exactement la règle des ±40 %), puis
  des frappes à 750 ms convergent (`67.6`, `70.8`) vers 80 ; l'étoile « non enregistré » apparaît à
  la première frappe mesurée.
- **Un piège de terminal que le harnais ne peut pas absorber** : `Terminal::clear` de ratatui
  demande au terminal où est le curseur (`ESC [ 6 n`) et **attend** la réponse, donc il échoue au
  bout de deux secondes devant un terminal muet. `Terminal::resize(Rect)` remet le tampon à zéro
  **sans rien demander** : c'est lui qu'on utilise pour « tout redessiner ».

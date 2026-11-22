# browse — les écrans

**Responsabilité.** L'écran plein écran `browse` : trois espaces (Bibliothèque, Chaîne, Set), la
chaîne éditée en blocs, les presets, le set de concert et le prompteur. `state/` est de la logique
**pure** (horloge fournie, testée sans terminal ni carte), `ui/` ne fait que dessiner (ratatui,
`TestBackend`), `actions/` **exécute** ce que l'état demande — c'est là que sont les effets de bord,
et nulle part ailleurs —, et `mod.rs` tient la boucle : les touches, l'horloge, les vumètres, le
dessin.

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

Un fichier par famille d'écrans, et `mod.rs` porte **les étages** : les communes (`S P n`), la
pédale (`* k`) et les sorties (`h`, l'autre espace, `q`) sont déclarées **une fois** et ajoutées à
la table de chaque espace dans cet ordre. Un écran **surcharge** une touche d'étage par une entrée
de sa liste `OVERRIDES` — d'autres mots, ou `bar: ""` pour la laisser à l'aide `h` — mais il ne la
**replace** pas : c'est ce qui fait qu'on la retrouve au même endroit d'un écran à l'autre. Les
écrans qui ne sont pas des espaces (les fenêtres, les réglages, l'édition d'un set, la scène)
gardent leur table à la main : ce qu'ils laissent passer est une **liste blanche**.

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

## Ce qu'on entend — `state/target.rs`, `plan.rs`, `build.rs`, `actions/sound.rs`

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

**L'entrée MIDI** arrive par `Start::midi` : un `midi_io::Input` déjà ouvert par le binaire, dont la
boucle vide la file **à chaque tour**, hors du fil audio, et dont chaque message passe par
`State::on_midi` (`state/midi.rs`, pure — ADR 0011). Trois choses à ne pas casser :

1. la file se vide **même quand l'action du tour est déjà prise** (sinon un top d'horloge
   attendrait) ; l'action qu'un message demande patiente dans `midi_pending` jusqu'à un tour libre,
   et **la dernière demande gagne**, comme pour les chaînes du thread de construction ;
2. un message **n'est pas une touche** : il n'a ni contexte ni propriétaire (ADR 0009) et aucune
   fenêtre ne le retient. La seule exception est `typing()` — on est alors au clavier, pas au pied ;
3. un `PC` emprunte `switch_to`, **le chemin du pied sur `PgDn`** : même banque préchargée, même
   fondu, même question quand la chaîne n'est pas enregistrée. Ne pas le court-circuiter.

**Avec un set**, les mêmes messages parlent du set (§ 12 de `set.md`) : `PC` = le morceau dont le
`pc` vaut n, `CC cc_sound` = le son k+1 du morceau joué, `Start` / `Stop` / `SPP` = la battue
(`set/beat.rs` : `beat_from_start`, `beat_stop`, `beat_seek_sixteenths`, qui passent par
`library::bars::BarMap` — elle seule sait qu'une section change de mesure). Deux pièges :

- **la fenêtre PC + CC** : un `PC` ne joue pas tout de suite, il se pose dans `State::midi_pc` et
  c'est `tick` qui le libère (`flush_midi_pc`) une fois la fenêtre passée. Un `CC` arrivé entre
  temps le consomme, si bien qu'Ableton — qui envoie les deux en tête de clip — ne provoque **qu'un**
  changement de son. Ne pas jouer le morceau depuis `midi_song` : ce serait deux installations ;
- un son hors du morceau, un `pc` qu'aucun morceau ne porte, une position au-delà du morceau :
  **ignorés avec un message**. Fractal écrête, namfoop le dit (ADR 0011).

Le **tempo** vient de là aussi (ADR 0008) : les tops d'horloge, `Start` / `Continue` / `Stop` vont à
l'arbitre, et le CC de tap appelle `tap_tempo_at` — **la touche `*`, au message près**. Chaque
message porte **son** instant (`on_midi(event, at_us)`), et c'est vital : la boucle ne repasse que
toutes les 30 ms, alors qu'un top vaut 20,8 ms à 120 BPM et qu'un tap horodaté au tour de boucle se
tromperait de 7 BPM. `[midi] clock = false` coupe l'horloge **sans** couper le tap.

La boucle **trace** chaque message (`midi PC 4`) et le port écouté est nommé dans la ligne d'état :
c'est ce qui rend « mon pédalier parle-t-il ? » vérifiable de bout en bout par
`scripts/check-e2e.py`, qui lui écrit dans un tube.

**Les paramètres vivants** (ADR 0007, ADR 0008) ne passent pas par là : la boucle tient les cases
`dsp::param::Control` du volume, du tempo et du clic, et les écrit d'après `st.plan()` **juste
après** le `match action` — pour qu'une chaîne installée par ce tour-ci les lise dès son premier
bloc — et les trace quand elles changent (`volume=±x.x`, `tempo=xxx.x`). Le thread de construction
les pose en `Volume::Live` / `Tempo::Live` dans **toutes** les chaînes, banque comprise.

**Le clic du métronome** suit le même chemin (`gain`, à zéro quand c'est éteint, et `downbeat` =
`Arbiter::beat_epoch`), plus **deux cases de phase** qui ne passent pas par le tempo : `clock_ticks`,
le compteur de tops que tient le fil de lecture MIDI (`midi_io::Input::clock_ticks`), et
`clock_phase`, écrite à chaque tour par `Arbiter::clock_phase`. Dès qu'une horloge est verrouillée,
le clic **compte les tops** au lieu de compter des échantillons : sa position devient absolue, donc
insensible à la dérive et aux changements de tempo. La phase est le **numéro du top d'ancrage**, que
le message a apporté (`midi_io::Message::clock_seq`) : une constante entre deux ancrages, donc
l'endroit du tour où la boucle l'écrit n'a aucune importance. `State::clock_phase_confirmed` dit
si l'ancre a été observée ou devinée, et l'en-tête l'annonce (« temps à confirmer : * »). `k` le bascule (`State::metronome`, éteint au démarrage, jamais dans un
preset : `unsaved()` ne le voit pas) et il ne va que dans les sorties déclarées **in-ear** par
`setup` (`OutputRoute::click`, posé par `setup/state.rs` d'après `OutRole::InEar`) : c'est ce qui
garantit qu'il n'atteint jamais le sondier. `State::metronome_routed` dit si une sortie le reçoit ;
sinon `k` renvoie à `setup` au lieu d'allumer un clic inaudible.

## L'exécution des actions — `actions/`

`state` **décide** et c'est testé sans terminal ni carte ; `actions/` **exécute**. La frontière est
la structure `Ctx` : elle énumère ce qu'une action a le droit de toucher (la bibliothèque, la
config, le thread de construction, la ligne d'état, le terminal), si bien qu'en voir un de plus
signale une décision en train de descendre dans l'exécution. Une famille par fichier — `sound.rs`
(la chaîne demandée, le grain), `files.rs` (correction d'un modèle, preset, mélange), `set.rs` (le
fichier du set, ses prompteurs, `$EDITOR`, le set du moment) — et un `match` **exhaustif** dans
`actions::run` : une action nouvelle ne compile pas sans dire qui la prend.

Chaque action rend un `Flow` : finir le tour, passer au suivant (rien n'a changé à l'écran), ou
quitter. C'est ce qui a remplacé les `continue` et les `return` au milieu de la boucle.

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
une horloge qui se tait. `*` fait **deux** choses, qui ne se refusent pas ensemble : écrire le tempo
(seulement hors du banc d'essai — la Bibliothèque n'est pas un preset) et recaler le **temps 1**, qui
ne touche à aucun preset et agit donc **partout** (`Arbiter::realign_downbeat`). Les refuser tous
deux laissait un clic qu'on ne pouvait plus recaler, le clic s'allumant lui depuis n'importe quel
écran.

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

`beat.rs` — **la battue** (§ 11) : la position dans le temps du morceau (`Beat` : la mesure, le
tick, et si elle avance ; la **section** est `prompt_section`, parce que le texte et la battue
avancent ensemble et que deux champs se désaligneraient), et `State::band()`, les **faits** que la
bande montre — `ui/set.rs` les dessine, il ne les calcule pas. La carte des mesures est pure et
vit dans `library::bars`. Quatre choses à ne pas casser :

1. **l'horloge est fournie** (`advance_beat(now_ms)`, appelée par `tick`) et **rien ne descend dans
   le fil audio** : le tempo d'une section entre dans l'arbitre par `set_section_tempo`, comme
   celui d'un preset, et **seulement au passage d'une section à l'autre et seulement s'il change** —
   sinon chaque mesure annulerait le tap, que le § 11 veut plus fort que le fichier. Il ne touche
   pas à `fx.chain.tempo_bpm` : un morceau qui module n'a pas à rendre son preset « non
   enregistré » ;
2. **en marche la position suit le temps, à l'arrêt elle suit le son** (§ 10, § 14) : c'est
   `follow_prompt` qui rend la main quand la battue tourne, et c'est tout le mécanisme ;
3. le rattrapage est **borné** (`CATCH_UP_TICKS`) : une image en retard ne perd pas de temps, mais
   une pause longue (`$EDITOR`, un modèle qui se charge) se **recale** sur l'horloge au lieu de
   faire défiler le morceau en accéléré ;
4. `Espace` est une touche **réglable** (`[set] prompt_play`), donc une variante de `KeySpec` comme
   les quatre autres : déclarée dans la **table** (`bindings/set.rs` et `bindings/stage.rs`), sans
   quoi les deux jointures de `keyboard.rs` échouent. `all_keys()` la trouve d'elle-même.

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
- la **bande de battue** (`band_lines`, § 11) ne tronque jamais : **quelque chose cède**. Ce qui
  cède, dans l'ordre : le rappel de la touche après `⏸` (la barre du bas la donne), l'étiquette de
  la section (le texte en dessous la marque), puis la mesure courante ; ce qui ne cède jamais, ce
  sont les **temps** — c'est la largeur des cases qui se serre, pour qu'un 7/8 montre ses sept
  croches sur 80 colonnes. « Allumé » est une **inversion** et non une couleur de plus, pour qu'un
  temps qui sonne reste reconnaissable comme temps 1 (jaune) ou comme autre temps (vert) ;
- `footer.rs` ne décide rien : il montre les touches de l'écran où l'on est, la première couche qui
  a la main gagne. `windows.rs` dessine l'**overlay d'aide** `h`, contextuel, par-dessus tout.

## Tester

La logique se teste dans `browse::state`, sans terminal. De bout en bout, `scripts/check-e2e.py`
pilote l'écran dans un pseudo-terminal (`--no-config --backend sim`, bibliothèque **jetable**
`--library <tmp>` : `browse` écrit les mélanges dans `<library>/cabs/`, ne jamais polluer la
bibliothèque de l'utilisateur) ; les pièges du pilotage y sont **implémentés**.

**La boucle elle-même se teste de bout en bout**, et c'est neuf : deux défauts de phase lui ont
échappé parce que les tests unitaires pilotaient le clic avec une phase déjà propre, alors que le
défaut était dans le calcul qui la produit et dans l'ordre de la boucle. `scenario_midi_horloge`
nourrit un port simulé depuis un **fil à part** (25 ms, soit 100 BPM : au rythme de la boucle de
lecture d'écran, la cadence allait de 40 à 120 ms selon la charge et l'horloge ne se verrouillait
pas) et exige une phase immobile. Faute de verrou, il **ne conclut pas** au lieu d'échouer.

La **bande de battue** est testée aux deux tailles d'écran du projet (80×25 et 160×50) dans
`ui/tests/beat.rs` : un écran à deux colonnes mêle les deux dans chaque ligne, et la set list
contient elle aussi le tempo du morceau — chercher `♩124` dans la ligne entière mesure donc la
mauvaise colonne. Les bordures `│` les séparent, et c'est le **dernier** segment qui porte la
bande.

`NAMFOOP_BROWSE_TRACE=<fichier>` écrit chaque chaîne réellement installée dans le thread audio
(modèle, IR, entrée, effets pré / post, taille de la table de vumètres), chaque grain mesuré par
l'éditeur, et les cases vivantes quand elles changent — dont `phase=N`, le top qui porte le temps.
Celle-là se trace parce qu'elle doit être **immobile** : une phase qui bouge fait reculer puis
avancer le temps déduit du compteur, et le clic double. C'est ce que vérifie le scénario
`scenario_midi_horloge` de `scripts/check-e2e.py`, et ce qu'on lit en premier quand un clic sonne
faux. **Pour ce qu'on entend, c'est elle qui fait foi** — ratatui n'écrit que les cellules
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
- **La bande de battue sur le Pi 5 : non mesurée.** Ce qu'il faut savoir du cadre : la boucle de
  `browse` redessine à chaque tour et scrute le clavier pendant **30 ms**, ce qui est aussi la
  granularité de la position — à ♩ 120 en 4/4 un temps dure 500 ms, et en 7/8 une croche 250 ms.
  L'écart visuel attendu est donc celui du rafraîchissement, pas celui de la battue.
- **Un piège de terminal que le harnais ne peut pas absorber** : `Terminal::clear` de ratatui
  demande au terminal où est le curseur (`ESC [ 6 n`) et **attend** la réponse, donc il échoue au
  bout de deux secondes devant un terminal muet. `Terminal::resize(Rect)` remet le tampon à zéro
  **sans rien demander** : c'est lui qu'on utilise pour « tout redessiner ».

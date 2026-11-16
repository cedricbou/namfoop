# namfoop — binaire CLI

**Responsabilité.** La ligne de commande (clap), les écrans plein écran (ratatui) et une fonction
mince par commande, qui appelle `engine` (audio), `library` (sons), `model` (config) et `dsp`.

**Fichiers.**
```
crates/namfoop/   binaire CLI
cli_audio.rs    options audio clap (AudioArgs, BackendArg) -> audio_io::Settings (CLI > config > défaut),
                to_audio (model::BackendKind -> audio_io::Kind) ; profil de carte (card_profile,
                detect, settings, base_routing : CLI > config > profil > défaut, ALSA seulement)
cli_routing.rs  options de routage clap (RoutingArgs, --route N=SOURCE) -> engine::Routing
tui.rs          commun aux écrans plein écran : Key, poll_key, Session (logs coupés, stderr
                redirigé, terminal restauré), meter, screen_text (tests)
browse/         `browse` : state/ (logique pure + horloge fournie, testée : keys.rs LE CLAVIER,
                seul endroit où l'ordre des couches est écrit — toute SAISIE DE TEXTE (nom, filtre
                des listes, filtre du catalogue de blocs) passe AVANT les touches communes
                (1 / 2 / S / P / n), sinon une lettre commune est volée au filtre et agit deux fois
                (#175 : le « n » de nam et de gain effaçait la chaîne) ; mod.rs état et
                navigation ; lists.rs listes des onglets Modèles / IR ; chain.rs la chaîne en blocs
                éditée (Leaf / Split / Branch, curseur 2D Cursor / Spot, dont Cursor::Merge : la
                SORTIE d'un parallèle, le ◇ de son bord droit, d'où `a` ajoute après lui — #152,
                inspiré du bloc Merge / Mixer du marché ; tout le reste du parallèle se règle depuis
                le ◆) ; label.rs étiquettes du
                synoptique ; edit/ navigation 2D et règles d'édition (s, +, x, [ ], { }, b, m/o/p) ;
                bands.rs LE PANNEAU BANDES (#183) : State::bands rend les 7 zones de
                library::profile::ZONES pour la chaîne ÉCOUTÉE (Item::voicing, le modèle NU, plus
                l'IR du moment via model_profile::with_ir — Item::profile, lui, contient déjà l'IR
                conseillée), ou l'ÉCART avec la référence ◆ ; mesure FIGÉE (profils en cache, aucune
                chaîne rebâtie) ; WIDE_COLUMNS = 128 est la source unique de SPECTRUM_MIN_WIDTH, et
                show_bands() suit `z` (State::bands) ou, à défaut, l'écran (State::width, posée à
                chaque image par la boucle) ;
                meters.rs vumètres du synoptique (+ level_align_db : L'ALIGNEMENT SUR LE DI de la
                chaîne installée, publié par dsp::meter::ChainMeters::level_db, #178 — il annule
                tout bloc `gain`, donc ui/synoptic.rs le montre grisé en bout de chaîne, le panneau
                de réglages prévient sur un `gain` (State::gain_cancelled_by_alignment) et l'ajout
                d'un `gain` le dit ; State::level_match vient de la config (level = "match")) ;
                space.rs les DEUX ESPACES (§ 2 de browse-espaces.md) : Space::Library (onglets
                Modèles / IR / Mélange, on entend le banc d'essai basse › modèle › IR SANS AUCUN
                effet) et Space::Chain (l'ancien onglet Effets, on entend la chaîne) ; 1 / 2
                changent d'espace et l'écoute suit, Tab et →← restent dans la Bibliothèque ;
                set/ LE SET ACTIF (#218, #219, § 4 et § 5 de docs/design/set.md) : ActiveSet porte le
                SetSpec lu par library::set et, pour chaque son, sa place dans State::presets
                (None : preset introuvable, le son est SAUTÉ). flat() est la liste à plat du § 5,
                step() / step_song() n'y BOUCLENT PAS (fin du set / début du set : on ne rejoue pas
                le premier morceau par mégarde), distinct_slots() donne la banque du set — UNE
                place par preset distinct, parce que le volume et le tempo sont vivants, donc hors
                de l'identité d'une demande de chaîne : c'est ce qui fait qu'un preset joué dans dix
                morceaux n'occupe qu'une place et ne rebâtit rien quand on passe d'un morceau à
                l'autre (vérifié dans la trace : place=0/1/2, `préchargée`). play_set_sound ajoute
                le volume du morceau à celui du preset et passe son tempo à l'arbitre comme le
                ferait un preset. set/screen.rs : L'ESPACE SET (#219) — le curseur (VISER N'EST PAS
                JOUER : ↑↓ ne changent rien à l'oreille, Entrée joue le morceau visé sur son son 1),
                `i` (plusieurs morceaux peuvent rester dépliés) et la fenêtre `L`, qui rend
                Action::ListSets puis OpenSet / SetChanged — la boucle seule voit la bibliothèque.
                Le set actif est retenu dans `[set] default` de la config, que `run --set` sans
                argument prendra aussi (#221) : une seule notion de « set du moment ». Attention,
                `load_preset` ne quitte PAS l'espace Set — jouer un son du set ne doit pas sortir de
                l'écran de concert. Le dessin est dans ui/set.rs (deux colonnes, trois largeurs :
                44 colonnes de set list au-delà de 100, 30 en dessous, une ligne compacte sous 70) ;
                preset/ les PRESETS (#110) : conversion chaîne <-> dsp::preset::PresetSpec (index
                de la bibliothèque <-> chemins relatifs + identifiant en filet), la banque `P`
                (presets déjà convertis : charger ne lit aucun fichier), `S` (nom proposé, écrit par
                browse/mod.rs via Action::SavePreset), l'indicateur de l'en-tête (unsaved() dont le
                preset est la référence : mark_preset_saved), `t` le TOML replié, la bascule qui
                demande une fois (confirm_switch) et `n` la CHAÎNE VIERGE (#170 : chaîne vide,
                preset DÉTACHÉ pour que `S` propose un nom neuf, mark_start sur la chaîne vide) ;
                les deux passent par pending_switch: Option<Pending> (Preset(i) | New), que
                browse/mod.rs reprend après l'écriture d'un preset ; LE VOLUME DU PRESET (#179,
                `,` / `.`, adjust_volume) vit dans chain::Chain::volume_db — avec les blocs, pour
                qu'unsaved() le voie sans code en plus. C'est un PARAMÈTRE VIVANT (#187, ADR 0007) :
                il n'est PAS dans Request (une demande décrit le son, pas le preset, donc une place
                de banque correspond quel que soit son volume) et ne demande AUCUNE chaîne ;
                browse/mod.rs tient la seule case (dsp::param::Control), l'écrit d'après
                st.plan().volume_db JUSTE APRÈS le `match action` — pour qu'une chaîne installée par
                ce tour-ci la lise dès son premier bloc — et la trace (`volume=±x.x`) quand elle
                change ; le thread de construction la pose en ChainSpec::volume = Volume::Live dans
                toutes les chaînes, banque comprise. `run --preset` (amp.rs), lui, garde un gain
                figé (Volume::Db). LE TEMPO DU PRESET (#195, ADR 0008) suit EXACTEMENT le même
                chemin : chain::Chain::tempo_bpm (avec les blocs, pour qu'unsaved() le voie sans
                code en plus), une seconde case posée par browse/mod.rs juste après le `match`
                (trace `tempo=xxx.x`), ChainSpec::tempo = Tempo::Live dans build.rs. C'est de lui
                que les paramètres `division` des effets de temps tirent leur durée. ATTENTION : deux
                valeurs distinctes depuis #200 — `fx.chain.tempo_bpm` est le tempo **enregistré**
                (ce que `S` écrit, ce que `unsaved()` compare ; seuls un tap et un chargement de
                preset le changent), et `State::heard_tempo_bpm()` est le tempo **entendu**, celui
                de l'arbitre `engine::tempo::Arbiter` que `State` possède. Les deux diffèreront dès
                qu'une horloge MIDI tournera (#7), et c'est l'entendu qui va dans la case et dans
                l'affichage. `*` = tap (`State::tap_tempo`, keys.rs), `tick()` appelle
                `Arbiter::poll` pour voir une horloge qui se tait ;
                send.rs Entrée y met le son du banc dans la chaîne (§ 4 : chaîne vide -> nam › ir,
                sinon la fenêtre « quel bloc remplacer », case « avec son IR », ou mode pose dans
                la Chaîne) ; heard_bench / chain_text pour la
                ligne d'écoute ; target.rs la chaîne telle qu'on l'écoute selon l'espace (plan :
                banc à nu, ou blocs édités avec l'essai du sélecteur et l'écoute isolée) ; picker.rs le sélecteur sur place d'un bloc nam / ir (§ 5 de
                browse-espaces.md : Entrée ouvre la liste sous le synoptique, ↑↓ essaie DANS la
                chaîne sans l'écrire — l'essai n'est posé que par plan() / shown_chain(), donc
                unsaved() l'ignore et Échap n'a rien à défaire ; o isole le bloc et son voisin,
                c bascule actuel / essai) ; meters.rs balistique des vumètres ; quit.rs : Échap ne quitte jamais (il remonte d'un niveau, dans
                `on_key`), `q` ouvre la fenêtre de confirmation (`confirm_quit`, `Q` seul quitte)
                quand `unsaved()` n'est pas vide, ou quand la chaîne est vide alors que le banc
                joue un son (`empty_chain_warning`, § 9 : Entrée le met dans la chaîne et reste,
                Q quitte quand même) — comparaison d'états via `mark_start` et
                `mark_blend_saved`, appelés par browse/mod.rs ; compare.rs A/B b / r et référence c via effective_ir / effective_model,
                editor.rs éditeur `e` avec écoute du gain et grain remesuré, blend.rs mélanges, fx.rs onglet
                Effets (fenêtre du catalogue — dessinée en DERNIER par ui/fx.rs, après le panneau
                du bas, sinon elle disparaît sur un bloc nam / ir (#148) —, réglages de l'élément sous le curseur — effet,
                parallèle ou branche —, écoute différée, v vumètres) ; navigation :
                Tab / Maj+Tab en boucle sur les TROIS onglets, → / ← de proche en proche
                (space.rs : switch_tab, testée dans les deux sens) ; l'affichage d'une valeur de
                réglage passe par dsp::fx::Unit::format (et non chain::num + symbol()) : une
                division se montre par son nom de note (`1/8.`, #195),
                tests.rs), ui/ (ratatui, TestBackend : mod.rs listes et barre d'état, spectrum.rs
                panneau ≥ 128 col. (#138 : police 10×20 du Pi), paramétré par courbes + référence (Bibliothèque : la référence
                épinglée ◆ ; sélecteur : l'actuel ●), editor.rs, fx.rs espace Chaîne, picker.rs (contenu entier du
                bloc nam / ir sous le curseur, et le sélecteur), rows.rs (une ligne de modèle /
                d'IR, partagée par les onglets et le sélecteur), synoptic.rs (le synoptique de la
                chaîne : une ligne par branche, vumètres au-dessus, colonnes alignées, défilement
                horizontal), tests.rs), mod.rs (index, Engine, boucle d'affichage,
                overrides.toml), build.rs (LE THREAD DE CONSTRUCTION : il possède la
                engine::bank::Bank — donc l'envoi de chaînes —, bâtit la chaîne demandée hors temps
                réel avec sa table de vumètres (dsp::meter::ChainMeters, #107), et PRÉCHARGE les
                chaînes des presets entre deux demandes (#6) : une bascule n'installe plus qu'une
                chaîne déjà prête. Une demande porte la place de banque dont elle est exactement la
                chaîne (Request::preset, égalité des blocs résolus) ; dès qu'on édite, isole ou
                essaie un son, elle vaut None et la chaîne est rebâtie. Job::Bank(reqs) rejoue le
                préchargement après chaque enregistrement de preset. `drain()` vide la file sans
                bloquer, la DERNIÈRE chaîne demandée gagne, et elle est rappelée JUSTE AVANT
                d'installer : une chaîne bâtie pendant que l'utilisateur a déjà rappuyé est JETÉE
                plutôt qu'installée (#191 — sinon on entend le preset d'il y a un ou deux appuis).
                Le préchauffage (bank.prewarm, 26 à 54 ms) se fait donc AVANT cette vérification, et
                l'installation par bank.install_warm), plan.rs (la chaîne demandée au
                thread de construction : chemins résolus, blocs coupés retirés, solo en muets) ; `import --review` l'ouvre sur les
                modèles importés (browse::Start) ; ui/windows.rs les fenêtres (« non enregistré »,
                « mettre dans la chaîne ») et l'OVERLAY D'AIDE `h`, contextuel, par-dessus tout
                l'écran ; ui/bands.rs le DESSIN du panneau Bandes (colonnes verticales, ±12 dB sur
                5 lignes, 3 dB par demi-bloc, ▲/▼ hors échelle ; sous la courbe du Spectre à partir
                de 128 colonnes, sinon sous la liste) ; ui/footer.rs LA BARRE DU BAS (les touches de l'écran où l'on est, la
                première couche qui a la main gagne) ; ui/mod.rs : help_bar la coupe aux
                séparateurs de touches et garde toujours les trois dernières (h aide / l'autre
                espace / q quitter), #150
setup/          `setup` plein écran : state.rs (logique pure, testée), ui.rs (ratatui, TestBackend),
                audio.rs (vumètres, bip, test des buffers ; engine::raw), devices.rs (cartes par backend)
live.rs         `run` : construit la chaîne, Engine::start, affichage (vumètre, charge DSP) ;
                FADE_MS / TRAILS_MS : le fondu et la QUEUE DE PRESET (#172, --no-trails la coupe),
                les mêmes constantes que browse/mod.rs
offline.rs      `render` (WAV -> chaîne -> WAV) et `bench`
latency.rs      mesure de latence par impulsion (câble de bouclage ; engine::raw) : le retour est
                le maximum du signal dans la fenêtre d'écoute, comparé au bruit de fond mesuré juste
                avant (pas de seuil fixe : PipeWire atténue et étale l'impulsion, #70) ; `Detector`
                est une machine à états pure, testée sans carte
import_cmd.rs   `import` : options clap, bilan affiché (profils calculés à l'affichage)
meta_cmd.rs     `meta` : options clap, affichage d'un modèle
models_cmd.rs   `models` : inventaire affiché
presets_cmd.rs  `presets` : la banque enregistrée (identifiant, nom, chaîne via BlockSpec::describe)
sets_cmd.rs     `sets` : les sets de concert (identifiant, nom, morceaux et sons), et SURTOUT ce qui
                ne va pas — un avertissement (preset renommé, prompteur absent) sous son set, une
                erreur bloquante à la place de la ligne. C'est la commande qui dit si un concert est
                prêt ; le format et les contrôles sont dans library::set (#217)
fx_cmd.rs       `fx [nom]` : catalogue des effets et paramètres (dsp::fx::catalog ; le parallèle
                n'est pas un effet) ; `--pre` / `--post` dans amp.rs,
                `bench --fx` dans offline.rs
live.rs (suite) `run --click` allume le clic ; il refuse de démarrer si aucune sortie ne le reçoit
                ATTENTION : `run --set` NE PASSE PAS par live.rs — c'est browse::run en mode
                VERROUILLÉ (State::locked, #221, § 9 de docs/design/set.md), parce que la scène est
                exactement la vue Set, privée de ce qui édite. keys.rs::locked_key est une liste
                BLANCHE (ce qui est permis) et non noire : une touche ajoutée ailleurs dans browse
                n'arrive donc jamais par accident sur scène. main.rs refuse AVANT d'ouvrir la carte
                quand aucun set n'est donné ni retenu : un service de scène doit échouer bruyamment
amp.rs          options CLI de la chaîne -> dsp::chain::AmpSpec (priorité CLI > config > défaut) ;
                `--tempo BPM` (ADR 0008) y pose AmpSpec::tempo_bpm, donc un Tempo::Bpm FIGÉ : `run`
                et `render` n'ont pas de tap, et c'est ce qui rend une mesure reproductible,
                `chain_spec()` : la chaîne à jouer, celle des options OU celle d'un preset
                (`--preset`, #159 ; les options de chaîne sont alors refusées, le cadre — trim,
                niveau, master, limiteur — reste celui de la config et des options),
                qui se traduit en chaîne en blocs (dsp::block::ChainSpec, ADR 0006) : --crossover HZ
                est un raccourci pour « bandes [HZ] : son direct | NAM › IR »
config.rs       glue de la config (types dans crates/model) : CalibrationExt (repère des vumètres)
calibrate.rs    `calibrate` : enregistre le DI (tap d'entrée d'Engine), calcule le trim, active le niveau « match »
```

**Interdit.** Ouvrir la carte autrement que par `engine` (`scripts/check-arch.sh` refuse tout
`audio_io::start`) ; mettre de la logique audio ou de bibliothèque ici (elle va dans la crate
concernée). Erreurs : `anyhow`.

**Documentation** (vérifiée par `scripts/check-docs.sh`) : textes d'aide clap complets (le test
`every_argument_has_help` échoue sinon), `scripts/gen-cli-doc.sh`, guide, README, CHANGELOG — voir
« Documentation » dans le `CLAUDE.md` racine.

## Tester

- `render`, `run`, `calibrate`, `latency` et `setup` lisent la config de l'utilisateur (carte,
  routage, trim...) : pour des résultats reproductibles (tests, comparaisons), toujours passer
  l'option globale `--no-config` (elle empêche aussi toute écriture).
- `setup` et `browse` sont plein écran : pour les tester de bout en bout, les piloter dans un
  pseudo-terminal (Python `pty`) avec `--no-config --backend sim` ; pour `browse`, une bibliothèque
  JETABLE (`--library <tmp>`) : il écrit les mélanges dans `<library>/cabs/`, ne jamais polluer
  `library/` de l'utilisateur. `NAMFOOP_BROWSE_TRACE=<fichier>` y écrit chaque chaîne réellement
  installée dans le thread audio (modèle, IR, entrée, effets pré / post, taille de la table de
  vumètres) et chaque grain mesuré par l'éditeur : c'est la vérification de ce qu'on entend. Chaque
  ligne est **horodatée** (ms depuis la première) et écrite en **un seul** appel, sinon les lignes
  des deux threads se coupent (#191) ; le fichier n'est lu dans l'environnement qu'**une fois**
  (#193). Une chaîne bâtie puis **jetée** parce qu'une demande plus récente est arrivée le dit aussi
  (`JETÉE`) : la trace répond toujours à « quelle chaîne joue vraiment ». Les lignes de diagnostic
  de #191 (une par touche, état de la banque, réponses de la banque) ont été retirées (#193) : les
  remettre au besoin est l'affaire de quelques lignes. La logique se teste dans `setup::state` /
  `browse::state`.
- Test de fumée : `namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.

## Faits mesurés

- **Bascule d'un preset** (`PgDn` / `PgUp` ; x86, profil `ci`, backend sim, mesurée en
  pseudo-terminal : de la touche à la ligne `NAMFOOP_BROWSE_TRACE`, écrite quand la chaîne est
  **dans** le thread audio) : **20 à 21 ms** depuis la banque préchargée (#167), pour les trois
  presets de test — y compris le parallèle à **deux** NAM. C'est le fondu, et c'est aussi le plancher
  de la mesure (la boucle d'affichage scrute le clavier toutes les 30 ms). Avant la banque (#158) :
  **81 ms** pour `nam › ir`, **82 ms** pour `gate › nam › ir › reverb`, **204 ms** pour le parallèle
  à deux NAM — le **chargement des modèles** dominait (≈ 80 ms le premier, ≈ 120 ms le second), pas
  le nombre d'effets. Le son ne se coupe dans aucun des deux cas : la chaîne précédente joue jusqu'au
  fondu. Un preset pas encore préchargé retombe sur l'ancien chemin. **Pi 5 non mesuré**
  (#44 / #45) ; coût mémoire et détail du préchargement : `crates/engine/CLAUDE.md`.

- **Le tap (`*`) marche de bout en bout** (#200). Mesuré en pseudo-terminal (`--no-config
  --backend sim --device 4x6`, bibliothèque jetable, `NAMFOOP_BROWSE_TRACE`) : charger un preset à
  `tempo_bpm = 92` écrit `tempo=92.0 (preset)`, deux frappes à 522 ms donnent `tempo=114.9 (tap)`,
  une 3ᵉ frappe une seconde plus tard **repart** une séquence (`59.3`, exactement la règle des
  ±40 %), puis des frappes à 750 ms convergent (`67.6`, `70.8`) vers 80. L'étoile « non
  enregistré » apparaît bien à la première frappe mesurée.
  LE CLIC DU MÉTRONOME (#201) : `k` le bascule (`State::metronome`, éteint au démarrage, jamais
  dans un preset — `unsaved()` ne le voit pas). Il va dans les sorties déclarées **in-ear** par
  `setup` (`OutputRoute::click`, posé par `setup/state.rs` d'après `OutRole::InEar`) : c'est la même
  information que l'assistant demande déjà, et c'est ce qui garantit qu'il n'atteint jamais le
  sondier. `State::metronome_routed` dit si une sortie le reçoit ; sinon `k` renvoie à `setup` au
  lieu d'allumer un clic inaudible, et `run --click` refuse de démarrer. browse/mod.rs tient les
  deux cases (`gain`, à zéro quand c'est éteint, et `downbeat` = `Arbiter::beat_epoch`), posées au
  même endroit que le volume et le tempo ; `run` (live.rs), lui, les fige.
  `*` n'agit que dans l'espace **Chaîne** (#207) : le banc d'essai n'est pas un preset, et y taper
  faisait apparaître « non enregistré » alors qu'on écoutait des modèles. `State::field_help` rend
  l'aide du réglage sous le curseur, **dynamique** pour une division (`1/4 = une noire, soit un
  temps (500 ms à 120 BPM)`) : la colonne de valeur du panneau ne fait que **9 caractères**, donc
  tout ce qui doit s'expliquer va dans cette ligne, pas dans la valeur.
  **Pièges pour écrire ces tests** : `os.kill(pid, 0)` **réussit sur un zombie**, donc il ne dit
  pas si le processus a fini — seul `waitpid(pid, WNOHANG)` le dit (j'ai cru à tort que `browse` ne
  quittait pas sur `q`/`Q`) ; un **emoji occupe deux cellules** du tampon ratatui, donc ne jamais
  coller une assertion à droite d'un emoji (`🔒 SET` se lit `🔒  SET`) ; dans un écran à **deux
  colonnes**, un `lines().find(...)` attrape la ligne ENTIÈRE, les deux colonnes mêlées (chercher
  la dernière, ou découper la largeur de la colonne).
  Il faut aussi poser la taille du pseudo-terminal (`TIOCSWINSZ`),
  sinon ratatui ne dessine rien du tout ; et **ne pas scruter l'écran pour une valeur qui change** —
  ratatui n'écrit que les cellules modifiées, si bien qu'un `♩ 92 (preset)` arrive en morceaux
  épars et qu'une recherche de texte retrouve l'ancienne valeur. La trace est la source de vérité.

- **Le tempo du preset arrive bien jusqu'au son** (#195, ADR 0008). Mesuré (`render --preset` sur
  une impulsion, `--no-limiter`, preset `tempo_bpm = 92` + `delay:division=1/8.`) : échos à
  **489,1 / 978,3 / 1467,4 ms** — la croche pointée à 92 BPM. `--tempo 120` sur le même preset :
  **375,0 ms**. C'était un **bug** du premier jet : `chain_spec()` ne reprenait que `volume_db` du
  preset, donc toute division retombait à 120 BPM quoi qu'il arrive. Le test
  `amp::tests::a_preset_brings_its_own_tempo_and_tempo_wins` le garde.
  Dans `browse`, un paramètre remplacé par une division se montre **entre parenthèses** avec la
  durée que le tempo lui impose (`(375 ms)`, `(2000 ms, plafond)`), et le régler est refusé **avec
  la marche à suivre** — ne jamais afficher un nombre qui n'agit pas (même esprit que #178).

- Niveau d'entrée : les captures Tone3000 n'ont pas d'`input_level_dbu`. Avec la basse de
  l'utilisateur dans l'iTwo (crête ~ -12 dBFS), il a fallu -18 dB d'entrée pour retrouver le son des
  démos (Bassman 800 « presque clair », THD < 5 % jusqu'à -6 dBFS de sinus) -> cible de calibration :
  crête robuste (99,9e centile) à -30 dBFS à l'entrée des modèles.

## Annexe : détail du harnais (reprise de l'ancien `CLAUDE.md` racine)

Le `CLAUDE.md` racine en garde l'essentiel ; voici le texte complet.

### Contexte

namfoop est un multi-effet basse temps réel en CLI (Rust) : modèles NAM + IR + effets, sorties vers
le sondier et les in-ears, presets pilotés par MIDI, cible PC Linux et Raspberry Pi 5. Le
propriétaire joue de la basse sur scène : **la fiabilité en live et la latence priment sur tout**.
Doc utilisateur : `README.md`. Plan initial : étapes 1 à 6 ci-dessous.

**Architecture : lire d'abord [`docs/architecture.md`](docs/architecture.md)** (crates, sens des
dépendances, règles vérifiées par `scripts/check-arch.sh`) et les décisions prises dans
[`docs/adr/`](docs/adr/) (ne pas les rouvrir sans nouvelle mesure). Découpage en cours : #47.

### Dépôt

```
scripts/check.sh          vérification sans matériel (À FAIRE PASSER AVANT TOUT COMMIT)
scripts/check-docs.sh     cohérence code <-> docs (appelé par check.sh)
scripts/check-arch.sh     sens des dépendances entre crates, table de règles en tête (appelé par check.sh)
scripts/gen-cli-doc.sh    régénère docs/cli.md
scripts/install-hooks.sh  hooks : pre-commit (refus sur main + check.sh --quick), commit-msg (Refs #n),
                          pre-push (refus vers main)
scripts/feature.sh        cycle issue -> branche -> PR -> merge (start | pr | merge | status)
scripts/test-feature.sh   teste ce cycle HORS LIGNE (dépôt jetable + faux gh) : branche réutilisée,
                          PR déjà fusionnée, fusion qui n'a pas eu lieu (#128) ; lancé par check.sh
docs/                     guide.md (utilisateur), cli.md (généré), architecture.md, adr/ (décisions)
.github/workflows/        CI : check.yml (check.sh --quick, sans bibliothèque de sons),
                          pr-rules.yml (issue référencée, nom de branche, CHANGELOG)
.github/actions/deps/     installation des dépendances système, avec 3 tentatives (#117 : un miroir
                          Ubuntu injoignable ne doit pas faire échouer la PR)
.github/ISSUE_TEMPLATE/   modèles d'issue (fonctionnalité, bug) ; pull_request_template.md
scripts/test-backends.sh  tests matériels sur les deux backends
library/          modèles .nam / IR .wav de l'utilisateur, docs/ des packs, cabs/, overrides.toml
                  (non versionné, ne pas modifier ni supprimer ; tests : bibliothèque jetable)
nam/              zips d'origine : Tone3000, komposition101 (achetés) — non versionnés, ne pas toucher
```

### Commandes

- Rust est dans `~/.cargo/bin` (pas toujours dans le PATH) : `export PATH="$HOME/.cargo/bin:$PATH"`.
- `scripts/check.sh` : fmt --check, clippy -D warnings, `cargo test` (debug), build release, rendu de
  chaque modèle de `library/` (échec si NaN ou silence ; signale les crêtes > +6 dBFS). `--quick` pour
  sauter la bibliothèque.
- Rendus de référence (`crates/dsp/tests/golden.rs`, lancé par `cargo test`, donc par
  `check.sh --quick` et la CI) : signal synthétique dans NAM seul, NAM + IR et la chaîne complète,
  blocs 16 / 48 / 64 / 1000, comparés à `crates/dsp/tests/golden/*.toml` (tolérance relative 1e-3,
  enveloppe ±0,05 dB ; un gain de 0,5 dB le fait échouer). Un déplacement de code ne doit JAMAIS
  les changer. Régénérer (`NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden`) seulement
  pour un changement de son voulu, en le disant dans la PR.
- `cargo test` **en debug** : `assert_no_alloc` n'est actif qu'en debug (ou dans le profil `ci`,
  qui garde les assertions). Le C++ NAM est toujours compilé en -O3 (build.rs).
- Profil `ci` (`NAMFOOP_PROFILE=ci scripts/check.sh`) : tests et binaire compilés une seule fois,
  sans LTO — c'est ce que fait la CI. Le binaire de scène reste `release` (LTO). Les scripts
  utilisent `target/$NAMFOOP_PROFILE/namfoop` (défaut `release`).
- `NAMFOOP_SKIP_CPP=1` : build.rs ne compile pas le C++ (clippy / check uniquement).
- CI (`.github/workflows/check.yml`) : jobs `lint` et `check` en parallèle, sccache (Rust + C++),
  `push` seulement sur main. Mesurer une étape : `gh run view <id> --log` (horodatages des `== `).
  Les dépendances système passent par `.github/actions/deps` (3 tentatives) : un échec réseau isolé
  chez Ubuntu ne fait plus échouer la PR. Si les 3 ratent, le message le dit — relancer le job
  (`gh run rerun <id> --failed`), ce n'est pas le code.
- `render`, `run`, `calibrate`, `latency` et `setup` lisent la config de l'utilisateur (carte,
  routage, trim...) : pour des résultats reproductibles (tests, comparaisons), toujours passer
  l'option globale `--no-config` (elle empêche aussi toute écriture).
- `setup` et `browse` sont plein écran : pour les tester de bout en bout, les piloter dans un
  pseudo-terminal (Python `pty`) avec `--no-config --backend sim` ; pour `browse`, une bibliothèque
  JETABLE (`--library <tmp>`) : il écrit les mélanges dans `<library>/cabs/`, ne jamais polluer
  `library/` de l'utilisateur. `NAMFOOP_BROWSE_TRACE=<fichier>` y écrit chaque chaîne réellement
  installée dans le thread audio (modèle, IR, entrée, effets pré / post, taille de la table de
  vumètres) et chaque grain mesuré par l'éditeur : c'est la vérification de ce qu'on entend. Chaque
  ligne est **horodatée** (ms depuis la première) et écrite en **un seul** appel, sinon les lignes
  des deux threads se coupent (#191) ; le fichier n'est lu dans l'environnement qu'**une fois**
  (#193). Une chaîne bâtie puis **jetée** parce qu'une demande plus récente est arrivée le dit aussi
  (`JETÉE`) : la trace répond toujours à « quelle chaîne joue vraiment ». Les lignes de diagnostic
  de #191 (une par touche, état de la banque, réponses de la banque) ont été retirées (#193) : les
  remettre au besoin est l'affaire de quelques lignes. La logique se teste dans `setup::state` /
  `browse::state`.
- Sans carte son, valider un changement audio avec `namfoop render` (même code que `run`) et
  `namfoop bench`. Écouter un rendu n'est pas possible pour un agent : s'appuyer sur les tests, les
  niveaux (crête/RMS) et des comparaisons numériques.

### Cycle de développement (obligatoire)

Toute évolution suit : **issue GitHub -> branche -> PR -> CI verte -> squash merge -> main à jour**.
La protection de branche GitHub n'est pas disponible (dépôt privé, offre gratuite) : le cycle est
imposé par `scripts/feature.sh`, les hooks git (`scripts/install-hooks.sh`) et le workflow
`pr-rules` de la CI.

```sh
scripts/install-hooks.sh                  # une fois par clone
scripts/feature.sh start 12               # reprend l'issue #12 -> branche feat/12-<titre>
scripts/feature.sh start "Titre" bug      # ou crée l'issue (labels : enhancement, bug, documentation,
                                          #   roadmap, dette technique, test matériel)
# ... commits (pre-commit : check.sh --quick ; « Refs #12 » ajouté au message) ...
scripts/feature.sh pr                     # check.sh complet, push, PR « Closes #12 »
scripts/feature.sh pr --refs              # idem mais l'issue reste ouverte (tests matériels à faire)
scripts/feature.sh merge                  # attend la CI, squash merge, supprime la branche, main à jour
scripts/feature.sh status
```

Règles :
- jamais de commit ni de push direct sur `main` (les hooks refusent ; `NAMFOOP_ALLOW_MAIN=1` est
  réservé aux urgences explicitement demandées par l'utilisateur) ;
- une branche = une issue ; le nom `feat|fix|docs/<n°>-...` porte le numéro (vérifié en CI) ;
- la PR référence l'issue (`Closes #n`, ou `Refs #n` si des tests matériels restent à faire) et met
  à jour `CHANGELOG.md` si elle touche au code (vérifié en CI et par `feature.sh pr`) ;
- fusion seulement CI verte (`feature.sh merge` attend et refuse sinon) ; squash uniquement ;
  `merge` ne dit « fusionnée » qu'après avoir vérifié que le commit de fusion est sur `origin/main`,
  et refuse une PR déjà fusionnée ; `start` ne réutilise jamais un nom de branche pris (#128) ;
- les tests matériels restants se cochent dans l'issue (label `test matériel`) ; un agent les
  demande à l'utilisateur ;
- les limites découvertes en route deviennent des issues (label `dette technique`) plutôt que
  des notes perdues.

### Documentation (vérifiée par le harnais)

Trois niveaux, tous en français :
- `README.md` : présentation et démarrage rapide ; cite chaque sous-commande ;
- `docs/guide.md` : guide utilisateur par tâche ; explique **chaque** option et sous-commande ;
- `docs/cli.md` : référence **générée** par `scripts/gen-cli-doc.sh` depuis les textes clap.
Plus `CHANGELOG.md` (section « Non publié »).

Pour toute fonctionnalité visible par l'utilisateur, dans le même commit :
1. textes d'aide clap complets (le test `every_argument_has_help` échoue sinon) ;
2. `scripts/gen-cli-doc.sh` ;
3. section du guide (le pourquoi, un exemple dans un bloc ```sh, les pièges) ; README si nouvelle
   sous-commande ;
4. une ligne dans `CHANGELOG.md` > « Non publié ».

`scripts/check-docs.sh` (lancé par `check.sh`) échoue si `docs/cli.md` est périmé, si une option ou
sous-commande manque au guide / README, si un exemple ```sh des docs ne passe pas l'analyse de la CLI
(mode `NAMFOOP_CHECK_ARGS=1`, qui vérifie aussi que les mots-clés -m / -i sont non ambigus quand
`library/` existe), ou si un `.nam` / `.wav` / `.zip` est versionné. Il avertit si du code a changé
sans entrée de changelog.

### Conventions

- Messages utilisateur, logs, docs et commentaires en **français**. Identifiants en anglais.
- Erreurs : `anyhow` dans le binaire, `thiserror` dans les crates bibliothèques.
- `rustfmt.toml` : largeur 110. Clippy sans warning.
- Ne jamais versionner `library/`, `nam/` ni de fichiers audio : les droits sur les modèles et IR
  Tone3000 ne sont pas établis (vérifié par check-docs.sh).
- Toolchain épinglée (`rust-toolchain.toml`) pour que clippy donne le même verdict en local et en CI.
- Mettre à jour ce fichier (architecture, règles, faits mesurés) avec le code.

### Feuille de route

Chaque étape a son issue (label `roadmap`) : `gh issue list --label roadmap`.

1. ✅ Base : backends JACK + ALSA (+ sim), canaux dynamiques, `setup`, `ports`, `latency`.
2. ✅ NAM + IR : nam-sys, dsp, `run`, `render`, `bench`, `models` ; calibration d'entrée, niveau
   « match », limiteur, compteurs d'écrêtage (prototype actuel).
3. Effets : gate, compresseur, EQ paramétrique, crossover LR4 (blend grave clair), chorus, delay, reverb.
4. Presets TOML + banque préchargée + changement de chaîne sans glitch (swap via `rtrb`, crossfade).
5. MIDI (JACK MIDI / ALSA seq) : Program Change -> preset, CC -> paramètres ; entrée micro sur l'entrée 2.
6. Live / Pi 5 : service systemd, démarrage auto, cross-compilation aarch64, bench sur Pi.

### Pistes ouvertes / dette connue

Suivies dans les issues GitHub (label `dette technique`) ; résumé :

- Pics de charge DSP en `run` (15–50 % max contre 3 % de moyenne au bench) : tester `mlockall`,
  gouverneur CPU `performance`, préchauffage des modèles.
- Limiteur sans lookahead (attaque instantanée) : colore les transitoires qui dépassent -1 dBFS ;
  un lookahead de ~1 ms serait plus propre mais ajoute de la latence.
- Crépitements signalés en JACK avec un modèle saturé : origine non confirmée (xruns ou grain).
- Pas encore de lissage des paramètres ni de changement de chaîne à chaud (prévu avec les presets).
- Le C++ NAM n'est pas couvert par `assert_no_alloc` (allocateur Rust seulement).

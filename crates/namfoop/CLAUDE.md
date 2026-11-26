# namfoop — binaire CLI

**Responsabilité.** La ligne de commande (clap), les écrans plein écran (ratatui) et une fonction
mince par commande, qui appelle `engine` (audio), `library` (sons), `model` (config) et `dsp`.

Les écrans de `browse` (espaces, clavier, chaîne éditée, presets, set) ont leur propre fichier :
[`src/browse/CLAUDE.md`](src/browse/CLAUDE.md), chargé quand on travaille dans ce répertoire.

**Fichiers.**

```
crates/namfoop/src/
main.rs         sous-commandes clap, et le refus AVANT d'ouvrir la carte quand `run --set` n'a ni
                set donné ni set retenu : un service de scène doit échouer bruyamment
cli_audio.rs    options audio clap (AudioArgs, BackendArg) -> audio_io::Settings (CLI > config >
                défaut), to_audio (model::BackendKind -> audio_io::Kind) ; profil de carte
                (card_profile, detect, settings, base_routing : CLI > config > profil > défaut,
                ALSA seulement)
cli_midi.rs     options MIDI clap (MidiArgs : --midi-in, --no-midi, --midi-channel) -> midi_io::Settings
                (CLI > config > défaut) ; `open()` ouvre le port **ou dit pourquoi il n'y en a pas**,
                et ne fait jamais échouer le démarrage — c'est le seul endroit où cette règle vit
                (check-arch.sh refuse `midi_io::open` ailleurs qu'ici et dans midi_cmd.rs).
                LES CANAUX ET LEURS NUMÉROS (ADR 0012) : `Listen` (la table canal -> `Numbers`,
                bâtie par `from_config`) ; `hears`, la seule règle « ce message est-il pour moi » ;
                `numbers(canal)`, ceux que browse (`State::on_midi_from`), run et midi_cmd
                appliquent ;
                `MidiIn` (ce que rend `open()`) l'applique en vidant la file, si bien que browse et
                run ne voient jamais un message d'un autre canal ; midi_cmd.rs, lui, montre tout et
                dit pourquoi il ignore (`Listen::why_not`). Sans canal, tout passe
cli_routing.rs  options de routage clap (RoutingArgs, --route N=SOURCE) -> engine::Routing
amp.rs          options CLI de la chaîne -> dsp::chain::AmpSpec (CLI > config > défaut) ;
                chain_spec() : la chaîne à jouer, celle des options OU celle d'un preset
                (--preset ; les options de chaîne sont alors refusées, le cadre — trim, niveau,
                master, limiteur — reste celui de la config et des options), traduite en chaîne en
                blocs (dsp::block::ChainSpec, ADR 0006) ; --crossover HZ est un raccourci pour
                « bandes [HZ] : son direct | NAM › IR » ; --tempo BPM (ADR 0008) pose
                AmpSpec::tempo_bpm, donc un Tempo::Bpm FIGÉ : `run` et `render` n'ont pas de tap,
                et c'est ce qui rend une mesure reproductible ; --pre / --post les effets
config.rs       glue de la config (types dans crates/model)
instrument.rs   LA CALIBRATION JOUÉE (docs/design/instruments.md § 4) : `resolve`, pure — (instrument
                demandé, carte) -> trim, enregistrement, cible, état ; --instrument > instrument par
                défaut de la bibliothèque > calibration de la config ; --no-config ne prend aucun
                instrument qu'on ne nomme pas. Résolue UNE fois dans main, puis passée à ceux qui
                lisaient la config : AmpArgs::calibration (#[arg(skip)], comme `library`),
                browse::Start::calibration, le Signal des profils de modèles. Ne JAMAIS la poser
                dans `conf.config` : `remember` (browse/actions/set.rs), l'écran MIDI et setup
                réécrivent la config chargée, et y porteraient l'instrument du jour (le scénario
                `scenario_instruments` de check-e2e.py compare la config à l'octet). La carte : le
                profil retenu, sinon le device ALSA réduit à un identifiant ; carte simulée et hors
                carte : la carte préférée, sinon la mesure la plus récente
instruments_cmd.rs `instruments` : la grille (fonction pure `grid`, testée), --adopt (la calibration
                de la config devient un instrument, config intacte), --default
library_dir.rs  OÙ EST LA BIBLIOTHÈQUE : --library > NAMFOOP_LIBRARY > clé `library` de la config >
                ./library s'il existe > model::config::data_dir()/library. Choisie UNE fois dans
                main, après la config ; les options clap portent `library_arg: Option<PathBuf>` et
                main pose le champ `library` (#[arg(skip)]) que lisent les commandes. `resolve` est
                pure et testée ; `Found::require` est l'erreur qui liste les dossiers essayés, pour
                les commandes qui n'ont rien à faire sans bibliothèque (browse, run --set, models,
                presets, sets, meta). `import` la crée ; `run`, `render`, `bench`, `latency` et
                `setup` s'en passent
theme.rs        LA liste des couleurs des écrans (et `paint` pour les commandes ligne à ligne),
                nommées par ce qu'elles signifient : les espaces (Bibliothèque, Chaîne, Set), les modes, les états, les vumètres, la chaîne en
                blocs. **Aucune couleur en dur ailleurs dans le binaire** — check-arch.sh refuse
                tout `Color::` hors de ce fichier. Des fonctions et non des constantes : la teinte
                se choisit à l'exécution. Deux usages qui partagent une teinte aujourd'hui gardent
                deux noms dès qu'ils peuvent diverger (le jaune du mode Édition et celui d'un
                vumètre qui chauffe). Les familles viennent de docs/identite/charte.md.
                Deux palettes, choisies une fois (OnceLock) : `COLORTERM` = truecolor / 24bit
                → teintes exactes de la charte, sinon les couleurs de base (colonne
                « Terminal » de la charte). Seuls les accents changent (text, popup_bg, inherit : jamais).
                En test, la palette est celle de base sauf dans `theme::with_palette` (par fil),
                et `tui::screen_colors` vérifie un écran entier dans les deux. Les dégradés
                (grain, Bandes, corps des vumètres) passent par `grade(Accent, Step)` : la rampe
                de l'espace (charte, « Les gradations ») ; seules les alarmes (meter_warn,
                meter_clip) en sortent
tui.rs          commun aux écrans plein écran : Key, poll_key, Session (logs coupés, stderr
                redirigé, terminal restauré), meter, screen_text (tests)
browse/         `browse` et `run --set` : voir src/browse/CLAUDE.md
setup/          `setup` plein écran : state.rs (logique pure, testée ; l'étape MIDI y liste les ports
                donnés par mod.rs et retient un `MidiChoice` — sans port en vue, elle n'écrit
                rien plutôt que de figer « pas de MIDI »), ui.rs (ratatui, TestBackend),
                audio.rs (vumètres, bip, test des buffers ; engine::raw), devices.rs (cartes par
                backend)
live.rs         `run` : construit la chaîne, Engine::start, affichage (vumètre, charge DSP), vide la
                file MIDI à chaque tour. Un PC n'y désigne rien — `run` joue UNE chaîne, il n'a ni
                banque ni set, et c'est dans les écrans qu'un PC choisit un preset — mais l'horloge
                et le tap (CC 14) vont à un `engine::tempo::Arbiter` et le CLIC les suit (ADR 0008).
                La boucle attend donc POLL_MS = 20 ms (un top vaut 20,8 ms à 120 BPM) et le vumètre
                est lu par tranches de DISPLAY_MS : `Engine::meters` remet les crêtes à zéro. Le
                tempo de la CHAÎNE reste figé ici : c'est ce qui garde `run` et `render`
                reproductibles. La PHASE du clic a ses propres cases (compteur de tops du port MIDI
                + Arbiter::clock_phase), comme dans browse. Le tempo ENTENDU va sur la ligne
                d'état, pas dans le journal, qui ne garde que les changements de SOURCE : une ligne
                par valeur publiée en donnerait des dizaines par seconde sur une horloge qui gigue
                (l'hystérésis republie à chaque croisement de sa bande) et elles se mêleraient à la
                ligne d'état, qui se réécrit sans retour chariot ;
                L'ACCORDEUR : la touche de [tuner] (lue par termios, sans écho ni mode ligne,
                SEULEMENT dans un terminal : Ctrl+C reste un signal et les journaux gardent leurs
                retours à la ligne) coupe la basse, comme le CC de l'accordeur ; un PC, le CC de son
                et `Start` la rendent ; la ligne d'état devient alors l'accordeur ;
                FADE_MS / TRAILS_MS, le fondu et la QUEUE DE PRESET (--no-trails la coupe), sont les
                mêmes constantes que browse/mod.rs ; `run --click` allume le clic, et refuse de
                démarrer si aucune sortie ne le reçoit
offline.rs      `render` (WAV -> chaîne -> WAV), `bench` et `bench --fx`
latency/        mesure de latence par impulsion (câble de bouclage ; engine::raw). mod.rs : les
                options, la session, le tableau des passes — la CHARGE n'y est donnée que sur la
                passe « à côté », la seule où la chaîne travaille sur du signal, et c'est elle que
                lit la ligne de conclusion ; rt.rs : ce qui tourne dans le callback
                — `Detector`, où la crête de la fenêtre d'écoute dit qu'un retour est là (comparée
                au bruit de fond mesuré juste avant : pas de seuil fixe, PipeWire atténue et étale
                l'impulsion) et son DÉBUT donne la latence (l'instant de la crête saute d'un lobe à
                l'autre) — le SEUIL de ce début doit sortir du bruit, sinon la fenêtre est rendue
                trop faible plutôt que mesurée sur un pic de bruit —, et `Runner`, qui enchaîne les
                passes des chaînes de référence (`dsp::bench`) en ne comptant ni les premières
                fenêtres ni leur charge, et qui renonce à une passe après MAX_WEAK fenêtres
                comptées **consécutives** sans retour (dispersées, elles disent un câble qui
                grésille, et l'affichage annonce toute passe écourtée). Une seule passe = une seule
                place, le trajet nu étant la seule sans chaîne : c'est le même type qui les porte,
                sans état impossible. Deux machines à états pures, testées sans carte et sous
                `assert_no_alloc` (la garde est dans `drive`, le pilote de test : tout test de la
                mesure vérifie du même coup la règle temps réel)
calibrate.rs    `calibrate` : enregistre le DI (tap d'entrée d'Engine), calcule le trim, active le
                niveau « match ». `Dest` dit où va la mesure : l'instrument (créé s'il n'existe
                pas, instrument::to_calibrate) sur cette carte, ou la config quand la bibliothèque
                n'a aucun instrument et qu'on n'en nomme pas
import_cmd.rs   `import` : options clap, bilan affiché (profils calculés à l'affichage) ;
                `--review` ouvre browse sur les modèles importés (browse::Start). Une source qui
                porte une valise (ou aucune source : la clé du dernier échange) part dans
                import_valise.rs, et une valise ne se mélange pas à des packs
import_valise.rs `import` d'une valise : le bilan (arrive / reste ici / à trancher), UNE touche
                (Entrée écrit, `a` abandonne ; sans réponse, on abandonne), l'écriture
                (library::exchange::import), la mémoire, l'export proposé dans la foulée. Un
                abandon n'écrit RIEN, pas même le nom de la machine. Refus avant tout : valise
                incomplète, format plus récent, fichier abîmé, namfoop qui joue ici
export_cmd.rs   `export` : options clap, bilan, place libre sur la clé (statvfs) ; l'écriture est dans
                library::exchange::suitcase
exchange.rs     commun à `export` et `import` : la mémoire des échanges (`echanges/` À CÔTÉ du
                fichier de config, jamais la config elle-même ; `--exchanges` sous --no-config), le
                nom de la machine (demandé une fois), l'heure locale notée, le dossier de la clé
                (celui du dernier échange sans argument), le bilan famille par famille
                (theme::paint : couleurs seulement vers un terminal) ; `Ui`, les questions (une
                ligne, une touche sans Entrée : `Terminal`, ou `Script` en test) ; `playing`,
                namfoop qui joue ici, cherché dans /proc (`browse`, `run`, `--review`) plutôt
                qu'un verrou qu'un arrêt brutal laisserait derrière lui
meta_cmd.rs     `meta` : affichage d'un modèle
midi_rules.rs   ce qu'un message demande à un set (le morceau d'un PC, le son d'un CC, la raison d'un
                refus) : règles pures, partagées par browse/state/midi.rs, qui joue, et l'écran MIDI,
                qui dit ce qui serait joué — une seule source pour « n'a que 2 sons »
midi_screen/    l'écran plein écran de `midi` (docs/design/midi.md), rangé comme setup/ : state.rs
                (pure, testée : brouillon de [midi], fenêtre « Régler » et sa capture, refus ;
                state/incoming.rs : ce qui arrive et ce que namfoop en ferait ;
                state/plan.rs : les onglets, le Plan du set — numéro d'un morceau par la règle de
                library::set::SetSpec::pc_owner, la même que l'édition du set de browse),
                fiche.rs (la fiche, fonction PURE : l'onglet, fiche-midi.txt et --fiche l'appellent ;
                lignes de 46 colonnes au plus, testé),
                ui.rs (neutre : theme::neutral, jamais d'accent d'espace), table.rs (les touches,
                déclarées une fois : barre et aide en dérivent), keyboard.rs + keyboard.ref.txt
                (l'oracle du clavier). Reçoit le port DÉJÀ ouvert par midi_cmd.rs, sans appareil
                décoché ni horloge imposée : il applique son brouillon lui-même
midi_cmd.rs     `midi` : devant un terminal, l'écran (midi_screen) ; sinon, ou --lines / --seconds,
                les ports d'entrée, puis les messages reçus en clair, préfixés de leur
                appareil, et les horloges écartées en sortant (`Horloge` résume les
                tops à une ligne par noire ; `en_clair` dit ce que fait le CC de l'accordeur, APRÈS
                la ligne brute, qui reste recopiable dans un port simulé). N'ouvre pas la carte son : c'est la commande des
                essais matériels MIDI
                Sa cible plein écran est spécifiée dans docs/design/midi.md
                (maquette : docs/design/midi.html)
models_cmd.rs   `models` : inventaire affiché
presets_cmd.rs  `presets` : la banque enregistrée (identifiant, nom, chaîne via BlockSpec::describe)
sets_cmd.rs     `sets` : les sets de concert (identifiant, nom, morceaux et sons), et SURTOUT ce qui
                ne va pas — un avertissement (preset renommé, prompteur absent) sous son set, une
                erreur bloquante à la place de la ligne. C'est la commande qui dit si un concert est
                prêt ; le format et les contrôles sont dans library::set
fx_cmd.rs       `fx [nom]` : catalogue des effets et paramètres (dsp::fx::catalog ; le parallèle
                n'est pas un effet)
```

**`run --set` ne passe pas par `live.rs`** : c'est `browse::run` en **mode verrouillé**
(`State::locked`, § 9 de `docs/design/set.md`), parce que la scène est exactement la vue Set, privée
de ce qui édite. `browse::state::keys::locked_key` est une liste **blanche** (ce qui est permis) et
non noire : une touche ajoutée ailleurs dans `browse` n'arrive donc jamais par accident sur scène.

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
- `setup` et `browse` sont plein écran : de bout en bout, `scripts/check-e2e.py` les pilote dans un
  pseudo-terminal, où les pièges du pilotage sont implémentés. La logique, elle, se teste dans
  `setup::state` / `browse::state`.
- Test de fumée : `namfoop --no-config --backend sim --device 4x6 run -q --seconds 2 --master-db -120`.
- Les rendus de référence (`golden`) et la règle du `cargo test` en debug sont décrits dans
  `crates/dsp/CLAUDE.md`.

## Faits mesurés

- **Niveau d'entrée** : les captures Tone3000 n'ont pas d'`input_level_dbu`. Avec la basse de
  l'utilisateur dans l'iTwo (crête ~ -12 dBFS), il faut -18 dB d'entrée pour retrouver le son des
  démos (Bassman 800 « presque clair », THD < 5 % jusqu'à -6 dBFS de sinus) -> cible de
  calibration : crête robuste (99,9ᵉ centile) à -30 dBFS à l'entrée des modèles.
- **Le tempo du preset arrive jusqu'au son** (ADR 0008), mesuré par `render --preset` sur une
  impulsion (`--no-limiter`, preset `tempo_bpm = 92` + `delay:division=1/8.`) : échos à
  **489,1 / 978,3 / 1467,4 ms** — la croche pointée à 92 BPM ; `--tempo 120` sur le même preset
  donne **375,0 ms**. Le contrat — un preset apporte son tempo, et `--tempo` l'emporte — est tenu
  par `chain_spec()` et gardé par le test
  `amp::tests::a_preset_brings_its_own_tempo_and_tempo_wins`.

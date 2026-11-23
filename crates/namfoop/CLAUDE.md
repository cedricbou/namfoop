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
cli_midi.rs     options MIDI clap (MidiArgs : --midi-in, --no-midi) -> midi_io::Settings (CLI >
                config > défaut) ; `open()` ouvre le port **ou dit pourquoi il n'y en a pas**, et
                ne fait jamais échouer le démarrage — c'est le seul endroit où cette règle vit
                (check-arch.sh refuse `midi_io::open` ailleurs qu'ici et dans midi_cmd.rs)
cli_routing.rs  options de routage clap (RoutingArgs, --route N=SOURCE) -> engine::Routing
amp.rs          options CLI de la chaîne -> dsp::chain::AmpSpec (CLI > config > défaut) ;
                chain_spec() : la chaîne à jouer, celle des options OU celle d'un preset
                (--preset ; les options de chaîne sont alors refusées, le cadre — trim, niveau,
                master, limiteur — reste celui de la config et des options), traduite en chaîne en
                blocs (dsp::block::ChainSpec, ADR 0006) ; --crossover HZ est un raccourci pour
                « bandes [HZ] : son direct | NAM › IR » ; --tempo BPM (ADR 0008) pose
                AmpSpec::tempo_bpm, donc un Tempo::Bpm FIGÉ : `run` et `render` n'ont pas de tap,
                et c'est ce qui rend une mesure reproductible ; --pre / --post les effets
config.rs       glue de la config (types dans crates/model) : CalibrationExt (repère des vumètres)
theme.rs        LA liste des couleurs des écrans, nommées par ce qu'elles signifient : les espaces
                (Bibliothèque, Chaîne, Set), les modes, les états, les vumètres, la chaîne en
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
                par valeur publiée en donnait des dizaines par seconde sur une horloge qui gigue
                (l'hystérésis republie à chaque croisement de sa bande) et elles se mêlaient à la
                ligne d'état, qui se réécrit sans retour chariot ;
                L'ACCORDEUR : la touche de [tuner] (lue par termios, sans écho ni mode ligne,
                SEULEMENT dans un terminal : Ctrl+C reste un signal et les journaux gardent leurs
                retours à la ligne) coupe la basse ; la ligne d'état devient alors l'accordeur ;
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
                niveau « match »
import_cmd.rs   `import` : options clap, bilan affiché (profils calculés à l'affichage) ;
                `--review` ouvre browse sur les modèles importés (browse::Start)
meta_cmd.rs     `meta` : affichage d'un modèle
midi_cmd.rs     `midi` : les ports d'entrée, puis les messages reçus en clair (`Horloge` résume les
                tops à une ligne par noire). N'ouvre pas la carte son : c'est la commande des
                essais matériels MIDI
                L'écran plein écran qui la remplacera est spécifié dans
                docs/design/midi.md (maquette : docs/design/midi.html)
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

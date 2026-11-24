# midi-io — entrée MIDI

**Responsabilité.** Ouvrir **une** porte d'entrée MIDI et livrer des messages typés, dans un fil à
part. Deux sources derrière le même type, comme `audio-io` a deux backends : un **port du
séquenceur ALSA** (pédalier USB, Ableton) et un **port simulé** qui lit des lignes de texte. Rien
d'autre : ce que les messages veulent dire est l'affaire de l'appelant.

**Interdit** (vérifié par `scripts/check-arch.sh`) : toute dépendance hors `alsa`, `libc`, `log`,
`rtrb`, `thiserror` — donc ni `clap`, ni `serde`/`toml` (la traduction des options et de la config
est dans `crates/namfoop/src/cli_midi.rs`), ni `dsp`, ni `engine`, ni aucune crate du projet.
`engine` **ne dépend pas** de cette crate : le MIDI n'entre pas dans la session audio, il est lu par
la boucle d'écran, qui le donne à l'arbitre de tempo (`engine::tempo`, machine pure) et à la banque
de chaînes. Un second contrôle textuel limite `midi_io::open` à deux fichiers du binaire
(`cli_midi.rs`, `midi_cmd.rs`) : partout ailleurs on reçoit un `Input` déjà ouvert.

**Fichiers.**

```
src/lib.rs    Event, Message (horodaté, numéroté en tops d'horloge, et son canal), Settings, Port,
              Input (la file, le fil, le compteur de tops), open()
src/seq.rs    port du séquenceur ALSA : liste des ports, choix par adresse ou par nom, abonnement,
              conversion des événements du séquenceur -> Event
src/sim.rs    port simulé « sim:<chemin> » : fichier ou tube nommé, lu sans bloquer
src/text.rs   le format texte d'un message (pc / cc / clock / start / stop / continue / spp, et
              « ch N » après pc et cc), lu par le port simulé ET affiché par `namfoop midi` — ce
              qu'on voit se recopie, canal compris
src/error.rs  erreurs thiserror ; les messages sont ceux que lit l'utilisateur
```

**Conventions MIDI** : [ADR 0011](../../docs/adr/0011-midi-conventions-fractal.md) (PC = morceau,
CC 34 = son, CC 14 = tap) et [ADR 0008](../../docs/adr/0008-tempo-et-divisions.md) (l'horloge).
Elles ne sont **pas** ici : cette crate ne sait pas ce qu'un numéro veut dire.

**Ce qui tient le temps réel.** Le fil de lecture n'est **pas** le fil audio : lire un port bloque,
et les règles temps réel de `crates/dsp/CLAUDE.md` l'interdisent dans un callback. `Input` tient donc son fil et
publie dans une file `rtrb` sans verrou ; l'appelant la vide au rythme de sa boucle. Une file pleine
**compte une perte** (`Input::dropped`) au lieu de bloquer, et le port se ferme au drop (le fil voit
l'ordre d'arrêt en 200 ms au pire, le délai de son `poll`).

**La position d'un temps est un numéro de top, et ce numéro ne se reconstitue pas.** D'où deux
choses, et il faut les deux :

- `Message::clock_seq` : **chaque message porte** le nombre de tops reçus à son arrivée — donc,
  pour un `ClockTick`, son propre numéro. C'est ce que lit le côté contrôle. Le déduire après coup,
  en comparant un compteur partagé à un comptage tenu ailleurs, obligerait à lire les deux au même
  instant : jamais tout à fait vrai, et un top compté entre les deux lectures décalerait la position
  d'un top — 17,6 ms à 142 BPM — sans que rien le signale.
- `Input::clock_ticks` : le même compteur en `Arc<AtomicU64>`, que le **fil audio** lit d'un `load`
  relâché pour savoir où en est l'horloge *maintenant*. Il ne peut pas passer par la file : vidée
  au rythme d'une boucle d'affichage, elle arrive dix fois trop tard pour placer un temps (un top
  vaut 20,8 ms à 120 BPM, la boucle repasse toutes les 20 à 30 ms). Le type est standard, donc
  `engine` n'a pas à dépendre d'ici — c'est le binaire qui relie les deux, comme pour les cases
  `dsp::param::Control`.

Un top est compté **avant** d'être poussé, donc compté même si la file débordait : perdre le
message ne coûte qu'une mesure de tempo, perdre le comptage décalerait la grille pour de bon. Et le
comptage **ne compte que des tops** : il croît indéfiniment, ne se remet à zéro ni sur `Start`, ni
sur `Stop`, ni après un silence, parce que décider qu'un silence invalide la phase ou que tel top
porte le temps 1 demande de savoir ce qu'un message *veut dire* — l'affaire d'`engine::tempo`.

**Choix dont la raison ne se lit pas dans le code.**

- **Un Program Change est numéroté de 1 à 128**, et non 0 à 127 comme le câble : c'est la
  numérotation du `pc` d'un set (`library::set::PC_MIN`), donc la seule que namfoop montre. La
  conversion est faite à l'entrée, une fois — `seq.rs` ajoute 1, le format texte lit 1 à 128 — pour
  qu'aucun consommateur n'ait à y penser.
- **Tous les canaux MIDI sont acceptés, et chaque message porte le sien** (`Message::channel`,
  1 à 16 ; `None` pour l'horloge, le transport, la position, et une ligne simulée sans `ch`). Le
  filtre n'est **pas** ici : décider qu'un canal est écouté demande de savoir ce qu'il veut dire,
  et `namfoop midi` doit pouvoir montrer un message qu'on ignore. Un pédalier sur son canal d'usine
  marche donc sans réglage. Le canal est converti à l'entrée (`seq.rs` ajoute 1), comme le PC.
- **Une valeur hors des 7 bits est ignorée**, pas écrêtée (contrôleur 14 bits, NRPN) : ce n'est pas
  un message qui nous parle, et l'écrêter inventerait un geste.
- Le **port simulé s'ouvre sans bloquer** (`O_NONBLOCK`) : un tube nommé sans écrivain bloquerait
  l'ouverture, et le fil ne verrait plus l'ordre de s'arrêter. La boucle relit donc à petits pas
  (1 ms), ce qui lit aussi la suite d'un fichier qu'on complète et ne coupe pas une ligne écrite en
  deux fois.
- **L'horodatage est un `Instant`**, pas un nombre de microsecondes : l'appelant tient déjà sa ligne
  de temps (la boucle de `browse` compte en millisecondes depuis son départ) et c'est lui qui
  convertit. Deux lignes de temps dériveraient.

## Faits mesurés

- **Gigue de l'horloge entrante** (Clavinova en DIN sur l'entrée MIDI de l'iTwo, PC de
  développement, `namfoop midi`, 142 BPM) : moyenne **142,03 BPM** sans dérive, dispersion
  **1,5 ms crête à crête sur la noire** (0,35 %), soit ±0,25 BPM sur la ligne affichée — qui est une
  mesure brute, noire par noire, sans filtrage. S'expliquent par la quantification du timer de
  l'instrument (17,6056 ms par top n'est pas entier), les trames USB de 1 ms de l'iTwo, les 320 µs
  par octet du DIN et le réveil du fil de lecture — l'horodatage étant pris quand ce fil voit le
  message, comme dit plus haut. `engine::tempo` l'absorbe entièrement : en deçà de 0,3 BPM le tempo
  publié ne bouge pas.

## Tester

`cargo test -p midi-io` : le format texte, le port simulé de bout en bout (fichier), un port absent
qui rend une erreur au lieu de paniquer, et `tests/seq.rs` — **le vrai chemin ALSA sans matériel**,
par le port de bouclage du noyau (« Midi Through ») : on y écrit, namfoop s'y est abonné, donc
l'abonnement et la conversion sont exercés pour de bon. Ce test **ne conclut pas** sans séquenceur
ni port de bouclage (conteneur, CI) : il le dit et passe.

Sans matériel, à la main :

```sh
mkfifo /tmp/m && namfoop --midi-in sim:/tmp/m midi      # puis, dans un autre terminal :
echo 'pc 3' > /tmp/m
```

Sur le matériel (utilisateur seulement) : brancher le pédalier, `namfoop midi`, appuyer sur une
pédale et lire les lignes ; Ableton donne l'horloge et le transport.

La place de cette crate dans le graphe des dépendances : la carte du `CLAUDE.md` racine et
[`docs/architecture.md`](../../docs/architecture.md).

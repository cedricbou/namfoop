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
src/lib.rs    Event, Message (horodaté), Settings, Port, Input (la file + le fil), open()
src/seq.rs    port du séquenceur ALSA : liste des ports, choix par adresse ou par nom, abonnement,
              conversion des événements du séquenceur -> Event
src/sim.rs    port simulé « sim:<chemin> » : fichier ou tube nommé, lu sans bloquer
src/text.rs   le format texte d'un message (pc / cc / clock / start / stop / continue / spp),
              lu par le port simulé ET affiché par `namfoop midi` — ce qu'on voit se recopie
src/error.rs  erreurs thiserror ; les messages sont ceux que lit l'utilisateur
```

**Conventions MIDI** : [ADR 0011](../../docs/adr/0011-midi-conventions-fractal.md) (PC = morceau,
CC 34 = son, CC 14 = tap) et [ADR 0008](../../docs/adr/0008-tempo-et-divisions.md) (l'horloge).
Elles ne sont **pas** ici : cette crate ne sait pas ce qu'un numéro veut dire.

**Ce qui tient le temps réel.** Le fil de lecture n'est **pas** le fil audio : lire un port bloque,
et les règles du `CLAUDE.md` racine l'interdisent dans un callback. `Input` tient donc son fil et
publie dans une file `rtrb` sans verrou ; l'appelant la vide au rythme de sa boucle. Une file pleine
**compte une perte** (`Input::dropped`) au lieu de bloquer, et le port se ferme au drop (le fil voit
l'ordre d'arrêt en 200 ms au pire, le délai de son `poll`).

**Choix dont la raison ne se lit pas dans le code.**

- **Un Program Change est numéroté de 1 à 128**, et non 0 à 127 comme le câble : c'est la
  numérotation du `pc` d'un set (`library::set::PC_MIN`), donc la seule que namfoop montre. La
  conversion est faite à l'entrée, une fois — `seq.rs` ajoute 1, le format texte lit 1 à 128 — pour
  qu'aucun consommateur n'ait à y penser.
- **Tous les canaux MIDI sont acceptés** et le canal n'est pas transmis : namfoop est un seul
  instrument, et un pédalier sur son canal d'usine doit marcher sans réglage. Le jour où une piste
  de la table parlera sur le même port, le filtre se posera ici (un champ de `Settings`).
- **Une valeur hors des 7 bits est ignorée**, pas écrêtée (contrôleur 14 bits, NRPN) : ce n'est pas
  un message qui nous parle, et l'écrêter inventerait un geste.
- Le **port simulé s'ouvre sans bloquer** (`O_NONBLOCK`) : un tube nommé sans écrivain bloquerait
  l'ouverture, et le fil ne verrait plus l'ordre de s'arrêter. La boucle relit donc à petits pas
  (1 ms), ce qui lit aussi la suite d'un fichier qu'on complète et ne coupe pas une ligne écrite en
  deux fois.
- **L'horodatage est un `Instant`**, pas un nombre de microsecondes : l'appelant tient déjà sa ligne
  de temps (la boucle de `browse` compte en millisecondes depuis son départ) et c'est lui qui
  convertit. Deux lignes de temps dériveraient.

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

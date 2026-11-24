# ADR 0012 · MIDI : des appareils et des canaux, sans rôles

**Statut : adopté** (décision de l'utilisateur). **Amende** l'[ADR 0011](0011-midi-conventions-fractal.md) :
ses numéros par défaut tiennent, mais ils appartiennent au **canal**, et l'horloge à **un
appareil**.

## Le problème

L'ADR 0011 parlait de « deux sources » : un pédalier au pied, Ableton sur la table. Sur scène, ce
découpage ne tient pas :

- les deux arrivent souvent par la **même prise** (Ableton relayé par le pédalier, sur l'entrée DIN) ;
- un troisième appareil s'invite vite — un clavier, les lights sur le câble du groupe ;
- le groupe impose ses numéros (le CC 20 pour les sons, par exemple), et le pédalier ne doit pas
  avoir à les envoyer aussi.

Donner des **rôles** (« le groupe », « mon pédalier ») à des prises obligerait à choisir une prise
par rôle, et perd son sens dès qu'un appareil de plus envoie du MIDI.

## La décision

**namfoop raisonne en appareils et en canaux.**

1. **Tous les appareils sont écoutés**, désignés par **leur nom** (l'entrée DIN, chaque appareil
   USB) ; un appareil branché en cours de route est repris, un appareil débranché ne bloque rien.
   Un appareil se **décoche** (`[midi] ignore`) : il est alors ignoré en entier. Nommer un port
   (`[midi] in`, `--midi-in`) restreint l'écoute à lui seul, comme avant.
2. **L'horloge et le transport appartiennent à un seul appareil** : ces messages n'ont pas de canal
   sur le câble, donc rien d'autre ne les rattache. Sans choix, c'est **le premier qui bat**, gardé
   tant qu'il bat (500 ms sans top, [ADR 0008](0008-tempo-et-divisions.md)) ; choisi
   (`[midi] clock_from`), c'est lui seul. Une horloge venue d'un autre appareil est **ignorée et
   signalée**, jamais additionnée.
3. **Les numéros appartiennent au canal** (`[[midi.channels]]`) : le canal 5 appelle les sons par le
   CC 20 pendant que le canal 1 garde le CC 34. Un numéro absent reprend celui de `[midi]`, dont les
   défauts sont ceux de l'ADR 0011. Sans canal réglé, tous les canaux sont écoutés avec les mêmes
   numéros ; dès qu'un canal est réglé, seuls les canaux réglés le sont.
4. **Le dernier message est suivi**, d'où qu'il vienne : pas de priorité, pas de mode secours. Si
   Ableton se tait, un tap au pied reprend le tempo (ADR 0008).

```toml
[midi]
clock_from = "UM-ONE"         # absent : le premier appareil qui envoie une horloge
ignore = ["Digital Piano"]    # absent : tous les appareils
[[midi.channels]]
channel = 5
cc_sound = 20
[[midi.channels]]
channel = 1                   # cc_sound absent : celui de [midi], 34 par défaut
```

**Compatibilité** : toute config écrite avant cette décision se relit et fait la même chose —
`in` n'écoute qu'un appareil, `channel` est un canal réglé avec les numéros de `[midi]`.

## Ce qu'on a écarté

- **Des rôles par prise** (« le groupe », « mon pédalier ») : ils supposent une prise par rôle, ce
  que la prise fusionnée contredit, et n'ont rien à dire d'un troisième appareil.
- **Des numéros par appareil** : un même canal venu de deux appareils aurait deux jeux de numéros,
  et le groupe, qui ne connaît que son canal, ne saurait pas lequel convenir.
- **Une priorité entre appareils** (Ableton passe avant le pied) : un changement fait au pied pour
  rattraper une erreur doit tenir. Contre l'appui par mégarde, un geste d'un canal se coupe.
- **Mêler deux horloges** : le tempo mesuré doublerait, et la grille du clic se décalerait pour de
  bon.

## Ce que ça change

- `midi-io` écoute tous les ports d'entrée du séquenceur (hors « Midi Through », qui ne s'écoute que
  nommé) et le port d'annonces du système ; chaque message porte son appareil et son canal ; le
  choix de l'horloge se fait dans son fil de lecture, avant le compteur de tops.
- Le binaire tient la table « canal → numéros » (`cli_midi::Listen`), appliquée par `browse`,
  `run` et `namfoop midi`.
- L'écran MIDI se décrit en appareils et en canaux ([`docs/design/midi.md`](../design/midi.md)).

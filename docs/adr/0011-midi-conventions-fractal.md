# ADR 0011 · MIDI : les conventions Fractal par défaut, Ableton pour le transport

**Statut : adopté** (décision de l'utilisateur). **Amende** la décision « MIDI à la Helix » du
§ 14 de [`docs/design/set.md`](../design/set.md) et complète l'[ADR 0008](0008-tempo-et-divisions.md)
(l'horloge et le tap).

## Le problème

namfoop doit répondre à deux sources MIDI, et une seule chose compte : **brancher sans rien
configurer**, un soir de concert, avec un pédalier au pied et Ableton sur la table.

- un **pédalier MIDI** change le morceau et le son ;
- **Ableton** donne le tempo et la position.

Or chaque fabricant a ses numéros. Choisir « le bon » n'a pas de réponse technique : c'est une
convention, et la bonne convention est celle que le matériel de l'utilisateur parle déjà.

## La décision

**On suit les conventions des pédaliers Fractal par défaut, et Ableton pour ce que Fractal ne
couvre pas.** Les numéros restent **réglables** par la config — le défaut est ce qui évite de
configurer, pas ce qui l'interdit.

| Ce qu'on reçoit | Message | Chez Fractal | Chez namfoop |
|---|---|---|---|
| le **morceau** | Program Change | preset | le morceau dont le `pc` vaut n, sur son premier son |
| le **son** du morceau | **CC 34**, valeur 0-7 | scene 1-8 | le son n+1 du morceau joué |
| le **tap tempo** | **CC 14** | tap tempo | une frappe de `*` |
| le **tempo** | Beat Clock (`0xF8`) | horloge | l'arbitre de tempo (ADR 0008) |
| la **position** | `Start` / `Stop` / `Continue`, SPP | — | le temps 1, la battue, la carte des mesures |
| le **set** | Bank Select **CC#0** + PC | banque de presets | plus tard (hors périmètre) |

## Pourquoi c'est la bonne convention, et ce qu'elle n'est pas

**« Compatible Fractal » n'est pas une carte MIDI fixe.** Les Fractal modernes (Axe-Fx III, FM3,
FM9) n'ont **aucun CC assigné d'usine** pour les scenes ou le tap : on les assigne dans
`SETUP > MIDI/Remote`. Seul **Bank Select CC#0 + Program Change** est fixe. Suivre Fractal veut donc
dire suivre les numéros que le monde Fractal utilise **de fait** : ceux de l'Axe-Fx II, que les
gabarits des pédaliers tiers (Morningstar MC6 / MC8, EurekaProm) répètent depuis — **CC 34** pour la
scene, **CC 14** pour le tap. C'est ce qui maximise la chance qu'un pédalier de gabarit Fractal
marche tel quel, et c'est tout ce qu'on peut promettre : le matériel qui ne les envoie pas se règle,
des deux côtés.

**La hiérarchie, elle, tombe juste.** Un set namfoop est `morceaux › sons`, et Fractal est
`presets › scenes` : le Program Change choisit le morceau, le CC choisit le son dedans. C'est la
même forme que chez Helix (`PC` › `CC 69`), le Quad Cortex (`PC` › `CC 43`) et Kemper (`PC` ›
`CC 50-54`) — seul le numéro change, et c'est précisément ce que cette décision fixe.

**Ableton couvre ce que Fractal ne donne pas** : il envoie l'horloge, `Start` / `Stop` /
`Continue` **et** le Song Position Pointer (en mode « Song »), et il reçoit le SPP. L'horloge et le
tap de l'[ADR 0008](0008-tempo-et-divisions.md) sont donc alimentés par la même source que la
scène. Un détail sans conséquence ici : la résolution du SPP d'Ableton plafonne à 1024 mesures, bien
au-delà d'un morceau.

## Les divergences volontaires

- **Une valeur hors du morceau est ignorée, et le dit.** Fractal **écrête** (une valeur au-delà de
  la dernière scene choisit la dernière). En concert, jouer silencieusement un autre son que celui
  demandé est pire qu'un message : on ignore, et l'écran explique.
- **Un tap détrône l'horloge** (ADR 0008), là où Fractal, Helix et le Quad Cortex verrouillent le
  tap dès qu'une horloge arrive. La raison tient toujours : on tape justement pour rattraper une
  désynchronisation du groupe.
- **Pas de SysEx.** Fractal passe par du SysEx pour ce que les CC ne couvrent pas (l'état d'un bloc,
  le tuner). namfoop n'en a pas besoin : ses gestes de scène tiennent en un PC et deux CC, et un
  format propriétaire ne se branche sur rien d'autre.

## Ce que ça change

- `docs/design/set.md` : le son d'un morceau est appelé par **CC 34**, et non plus par CC 69. La
  décision du § 14 (« MIDI à la Helix ») est amendée ici.
- L'[ADR 0008](0008-tempo-et-divisions.md) annonçait « un CC de tap » sans numéro : c'est **CC 14**.
- Le choix du **set** par MIDI, quand il viendra, passera par **Bank Select CC#0** et non par le
  CC 32 des setlists Helix.

## Ce qu'on a écarté

- **Helix** (`CC 32` setlist › `PC` › `CC 69` snapshot) : c'était la convention retenue le
  15/11/2026, sur l'état de l'art. Elle n'avait pas de meilleure raison que Fractal, et
  l'utilisateur branche du matériel de gabarit Fractal : le défaut suit le matériel.
- **Un numéro « neutre »** choisi hors des conventions du marché : il garantirait un réglage, chez
  tout le monde, pour ne satisfaire personne.
- **Accepter les deux** (CC 34 **et** CC 69 pour le son) : deux portes d'entrée pour un même geste,
  donc deux fois la surface à expliquer et à tester, pour un gain nul — le numéro est réglable.

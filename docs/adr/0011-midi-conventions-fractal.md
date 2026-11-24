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
| l'**accordeur** | **CC 15**, ≥ 64 / < 64 | tuner | coupe la basse et l'affiche / rend le son |
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
- **Amendement — l'accordeur, CC 15** : le numéro de l'accordeur sur l'Axe-Fx II, de la même famille
  que le CC 14 du tap. Ce n'est pas une bascule : 64 ou plus coupe la basse, moins de 64 rend le son
  (Fractal envoie 127 puis 0), et un Program Change, un CC 34 ou un `Start` le rendent aussi
  ([`accordeur.md`](../design/accordeur.md) § 3). Rien ne garantit qu'un gabarit Fractal de
  pédalier tiers l'envoie déjà : à vérifier sur le matériel.
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

## Amendement : trois gestes sans convention à suivre

**Adopté** (décision de l'utilisateur, 24/11/2026), avec la spécification
[`instruments.md`](../design/instruments.md).

Un pédalier à quatre boutons, sans Ableton, a besoin de trois gestes que les tableaux ci-dessus ne
couvrent pas : dire **quel instrument** on a pris, passer au **morceau précédent**, passer au
**morceau suivant**. La règle de cet ADR est de suivre le numéro que le matériel parle déjà. Pour
ces trois gestes, il n'y en a pas :

- **L'instrument.** Le concept existe : des réglages d'entrée nommés par instrument (Boss GX-100 et
  GX-10, « INPUT SETTINGS » 1 à 10), des préréglages de la section d'entrée par guitare (Kemper).
  Aucun numéro MIDI convenu pour les rappeler n'a été trouvé dans la documentation de Boss, de
  Kemper, de Fractal et du Quad Cortex.
- **Le morceau suivant / précédent.** Chez Fractal, un preset se rappelle par Program Change : le
  wiki Fractal ne liste aucun CC « preset suivant ». Les CC 123 et 124 de l'Axe-Fx II incrémentent
  la *scène*, et ce sont aussi les messages de mode « All Notes Off » et « Omni Off » de la norme
  MIDI : les reprendre exposerait à un changement de morceau quand un séquenceur les envoie. Sur le
  Quad Cortex, un incrément est encore réclamé par ses utilisateurs ; l'Ampero II Stage de Hotone a
  les siens (CC 26 et 27), propres à cet appareil.

| Ce qu'on reçoit | Message | Chez namfoop |
|---|---|---|
| l'**instrument** | **CC 102**, valeur n | l'instrument de numéro n+1, après confirmation par le même geste |
| le **morceau précédent** | **CC 103**, ≥ 64 | le morceau d'avant dans le set, sur son premier son |
| le **morceau suivant** | **CC 104**, ≥ 64 | le morceau d'après |

- **102 à 104** sont dans la plage 102-119, que la norme MIDI laisse sans affectation, et loin des
  numéros des trois gabarits livrés. Ils restent réglables, par canal, comme les autres.
- **64 ou plus agit, moins de 64 est ignoré** : c'est la règle des incréments de scène de Fractal,
  et ce qui fait qu'un bouton qui envoie 127 à l'appui et 0 au relâcher ne compte qu'une fois.
- **La valeur de l'instrument compte de 0**, comme celle du CC 34 des sons.
- **Les gabarits livrés ne portent pas ces numéros** : ils décrivent ce que parle un appareil, et
  aucun ne parle ces gestes.

Le refus d'« un numéro neutre choisi hors des conventions du marché », plus haut, tient toujours
pour les gestes qui **ont** une convention. Ces trois-là n'en ont pas : le numéro de namfoop est
alors le seul possible, et le rendre réglable est tout ce qu'on peut offrir.

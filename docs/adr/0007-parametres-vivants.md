# ADR 0007 · Paramètres vivants : une case partagée, écrite par le contrôle

**Statut : adopté** (décision de l'utilisateur, 15/11/2026, #187). **Précise l'[ADR 0005](0005-effets-et-chaines.md)**
sur le « en deux temps » des paramètres, sans le contredire : le premier paramètre vivant existe, le
`set_param` général reste à faire (#6).

**Contexte.** Le volume d'un preset (#179) mettait **82 à 390 ms** à s'entendre : la valeur faisait
partie de l'identité de la chaîne demandée, donc la changer rebâtissait tout — chargement des
modèles NAM, des IR, et repassage du DI de calibration pour mesurer un alignement qui ne pouvait pas
changer (#187). L'utilisateur a réglé son fuzz à l'oreille, par tâtonnement, avec ce décalage ; #177
décrit le même mal sur les réglages d'effet, où « comme chaque appui installe une chaîne, on croit
facilement que le réglage ne fait rien ». D'autres réglages vivants viendront : tempo, marche /
arrêt d'un bloc, CC MIDI vers un paramètre.

Jusqu'ici, **rien ne circulait du contrôle vers le thread audio** : le seul véhicule était une
chaîne entière (`rtrb` de capacité 1, `dsp::swap`), et tous les atomiques du projet allaient dans
l'autre sens — l'audio publie, l'écran lit (`dsp::meter`, vumètre du limiteur, ADR 0006).

**Décision.**
- **Le contrôle possède la valeur, le temps réel la lit.** Une case préallouée (`dsp::param::Control`
  : un `f32` dans un `AtomicU32`), lue **une fois par bloc** par l'étage qui s'en sert. C'est le
  miroir de `dsp::meter`, dans l'autre sens, et le même patron que l'ADR 0006 a déjà retenu pour les
  mesures : aucune allocation, aucun verrou, aucun log en temps réel.
- **Pas de file de commandes** (l'ADR 0004 interdit déjà un bus de messages générique). Avec une
  case, une chaîne rebâtie — ou préchargée depuis dix minutes dans `engine::bank` — prend la bonne
  valeur **dès son premier bloc**, sans rien rejouer ; une file obligerait à rejouer l'état à chaque
  bascule de preset.
- **Un seul écrivain par case**, et c'est l'état de l'écran. Un CC MIDI, un tap tempo ou un
  footswitch entrent dans l'état **comme une touche** ; s'ils écrivaient la case directement,
  l'image suivante l'écraserait. L'écriture vit dans la boucle de `browse`, jamais dans
  `browse::state`, qui ne tient aucune poignée `dsp`.
- **Une case par paramètre, partagée par toutes les chaînes audibles**, la queue de preset comprise
  (#172). Un paramètre vivant n'appartient donc **pas** à un preset en temps réel, seulement à
  l'enregistrement : pendant un fondu les deux chaînes lisent la même valeur (c'est ce qu'on veut,
  pas de saut de niveau), et une réverbe qui finit de sonner suit le réglage. Le remède — une case
  par place de banque — coûterait une table à tenir et rouvrirait la question « laquelle écrit
  quoi » : refusé.
- **Lecture une fois par bloc, et rampe de durée fixe** quand la valeur saute : 20 ms comptées en
  **échantillons**, qui traversent les frontières de bloc (`gain::LiveGain`, patron de
  `Delay::set_time` et de `SwapChain`). Jamais une rampe étalée sur le buffer : le son dépendrait de
  la taille de bloc (48 en ALSA, 64 en JACK, 1000 en `render`), ce que les règles temps réel de
  `crates/dsp/CLAUDE.md` interdisent.
- **La valeur n'entre pas dans l'identité d'une demande de chaîne** : elle ne rebâtit rien et ne
  disqualifie pas une place de banque préchargée. Autrement dit, une demande décrit **le son**, pas
  le preset.
- Un étage qui lit une case s'en **protège** : valeur non finie ignorée, et rien à recalculer quand
  les bits lus sont ceux du bloc précédent (en régime établi, le coût redevient celui d'un gain
  fixe).
- Deux espèces de réglages, et la frontière est celle du **mécanisme** :
  1. **ce que le temps réel peut lire** (volume, mix, gain de branche, fréquence d'un filtre, temps
     de delay, tempo) → une case. Si l'étage doit redériver des coefficients ou déplacer une tête de
     lecture, il le fait une fois par bloc, **un paramètre à la fois, avec mesure** ;
  2. **ce qui change la structure** (modèle, IR, ajout ou retrait d'un bloc, marche / arrêt) → le
     chemin existant : reconstruction hors temps réel, banque préchargée, fondu. Le marche / arrêt
     d'un bloc n'est déblocable qu'après `Stage::tail_ms` (#173), sinon on coupe la queue d'un delay
     en le contournant.
- La case ne **décrit** rien : ni nom, ni unité, ni plage. La description reste `fx::ParamInfo`
  (ADR 0005), et c'est elle qui engendrera les cases quand les paramètres d'effet deviendront
  vivants (#6) — ici, **une** case faite à la main, pour le volume du preset.

**Conséquences.** `,` / `.` s'entendent au bloc suivant au lieu de 82 à 390 ms, et la banque
préchargée recorrespond (bascule de preset toujours à 21 ms). Un paramètre vivant ne peut pas être
borné par le temps réel : les bornes sont celles de l'écran qui écrit. Deux presets qui ne diffèrent
que par leur volume deviennent la même demande, donc la trace peut nommer une autre place de banque
que l'en-tête. Ce que cet ADR ne couvre pas : la modulation entre blocs et l'automation, que
l'[ADR 0006](0006-chaine-en-blocs-et-parallele.md) refuse toujours, et un paramètre par chaîne.

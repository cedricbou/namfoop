# ADR 0010 · Un seul backend de scène : ALSA direct, plus la carte simulée

**Statut : adopté.** Remplace l'[ADR 0001](0001-alsa-direct-sur-scene.md).

**Contexte.** L'ADR 0001 gardait JACK « pour le développement et les essais ». À l'usage, il n'a
servi ni à l'un ni à l'autre : les sessions de travail se font en ALSA direct, les tests d'agent sur
la carte simulée, et les essais matériels sur la carte de scène. Un backend que personne ne lance
n'est pas gratuit :

- il imposait les en-têtes `libjack` (`pkg-config`) à la compilation de **tout** le projet, sur
  chaque poste, en CI, et il aurait fallu les croiser pour la cible aarch64 du Pi 5 ;
- il doublait la surface à vérifier à chaque changement audio, alors qu'il n'était jamais vérifié ;
- il portait une dette non diagnostiquée (des crépitements avec un modèle saturé, origine jamais
  établie) qui ne pouvait pas se refermer faute de quelqu'un pour la reproduire.

**Décision.** namfoop ouvre la carte en **ALSA direct** (`hw:`), sans serveur audio, en concert
comme en travail. La **carte simulée** (`--backend sim`) reste le second backend : elle exerce toute
la chaîne temps réel sans matériel, et c'est le seul backend qu'un agent peut lancer. Il n'y a plus
de backend JACK.

**Conséquences.** Le backend ALSA porte la gestion des xruns, des streams liés, de la négociation de
format et du profil PipeWire mis à « off » le temps de la session. Tout changement audio se vérifie
sur **ALSA** (matériel, `scripts/test-backends.sh`) et sur **sim** (sans matériel, donc par un
agent). `audio-io` ne dépend plus que d'`alsa`, `libc`, `log` et `thiserror` — en particulier plus de
`serde`, ce qui rend vérifiable par `scripts/check-arch.sh` la règle « `audio-io` ne connaît pas la
config ». La sous-commande `namfoop ports` et l'option `--client-name`, qui n'existaient que pour
JACK, disparaissent. Une config qui porte encore `backend = "jack"` est refusée avec la ligne en
cause et la valeur à mettre.

**La couture reste.** `trait Io` / `Processor` / `Stream` et le `match` unique de
`audio_io::start_with` ne bougent pas : deux implémentations réelles les justifient, et aucune crate
métier ne nomme un backend. Rajouter un backend demain coûte **un fichier** dans `audio-io`, **une
variante** dans chacune des trois énumérations (`audio_io::Kind`, `model::config::BackendKind`, et
`BackendArg` pour l'aide de la CLI) et **un bras** du `match` — les `match` étant exhaustifs, le
compilateur nomme lui-même les endroits. Les trois énumérations ne sont pas une triplication à
corriger : `BackendArg` est un `clap::ValueEnum` dont les commentaires de variantes produisent les
blocs « Possible values » de l'aide et de `docs/cli.md`, et la règle d'orphelin interdit de le porter
sur un type d'une autre crate.

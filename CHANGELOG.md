# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Chaque fonctionnalité ou
correction visible par l'utilisateur ajoute une ligne dans « Non publié » ; à la publication, la
section prend le numéro de version.

## Non publié

### Ajouté (#34)
- Profil sonore des IR et mélanges : réponse par tiers d'octave (IR telle qu'elle est jouée),
  mini-spectre de 12 colonnes et étiquettes relatives au pack (« sub +4 », « sombre », « air +3 »,
  « aigus coupés à 3,9 kHz »), affichés par `models` et dans l'onglet IR de `browse`. Calculé à
  l'import, gardé dans `library/.cache/ir-profiles.json` (par empreinte du contenu).
- Ordre naturel des fichiers (Mix 2 avant Mix 10).
- `browse` accepte une bibliothèque sans modèle (écoute des IR sur le son direct).

### Ajouté (#30)
- `browse` : ligne « son direct (DI, sans modèle) » en tête de l'onglet Modèles, pour écouter les
  IR seules sur le son de la basse ; la commande affichée en sortie est alors `namfoop run -i <id>`.

### Corrigé (#28)
- `import` : un fichier identique à un autre (même contenu, comparé octet par octet après
  l'empreinte) est désormais importé et signalé au lieu d'être écarté, pour que chaque pack reste
  complet (série de micros d'un baffle). `--skip-duplicates` rétablit l'ancien comportement. Un
  réimport compte les fichiers « déjà présents » ; un fichier différent du même nom est refusé.

### Ajouté (#26)
- Guide : tutoriel d'import pas à pas (import et revue, type de capture à l'oreille, gain
  d'entrée par rapport à la calibration, volume en mode `match`, corrections en masse).

### Modifié (#26)
- `browse`, éditeur : `a` n'applique aux modèles filtrés que les champs modifiés (type et/ou
  entrée), au lieu de recopier les deux.
- Guide : `--output-db` est sans effet en mode `match` (l'alignement de niveau vient après).

### Ajouté (#21)
- `browse` : touche `e`, éditeur du modèle en écoute — type de capture, gain d'entrée propre
  (entendu tout de suite : calibration à l'oreille), IR préférée (les conseillées défilent en
  direct), marque, nom ; `s` enregistre dans `library/overrides.toml`, `a` applique le type et
  l'entrée à tous les modèles filtrés. Modèles corrigés marqués ✎, réduction du limiteur affichée.
- `namfoop import --review` : ouvre le navigateur sur les modèles importés, éditeur ouvert.
- `namfoop browse --filter` : filtre de départ.

### Ajouté (#20)
- `namfoop import` : importe zips, dossiers et fichiers (rangement par pack avec sous-dossiers,
  notices dans `library/docs/`, fichiers parasites ignorés), écarte les doublons par contenu,
  vérifie chaque modèle (chargé et joué) et chaque IR, déduit le type de capture du nom et des
  dossiers (`DI`, `FullRig`…) quand les métadonnées sont fausses, retient l'IR fournie avec le
  pack comme IR préférée. Remplace `scripts/import-tone3000.sh`.
- `namfoop meta` : affiche et corrige les métadonnées d'un modèle (type de capture, marque, nom,
  gain d'entrée propre, IR préférée), aussi en groupe (`--all`). Corrections dans
  `library/overrides.toml`, par empreinte du contenu : les fichiers d'origine ne sont jamais modifiés.
- Les valeurs de remplissage des métadonnées (`T3K-Null`, `tz-make`…) sont traitées comme absentes.

### Modifié (#20)
- Le « pack » d'un fichier inclut ses sous-dossiers (`PopPunkBass_NAMA2/DI`).

### Ajouté (#16)
- `namfoop browse` : navigateur plein écran qui joue en direct — modèles (badge IR incluse /
  requise), IR conseillées en tête, filtre, écoute après 150 ms d'arrêt, création de mélanges à
  l'oreille (gain, polarité, alignement) sauvés dans `library/cabs/`, commande équivalente à la
  sortie.
- Changement de chaîne à chaud sans clic (`dsp::swap`, fondu de 20 ms), réutilisable par les presets.

### Ajouté (#15)
- Mélanges d'IR (`library/cabs/*.cab.toml`) : plusieurs micros / positions avec gain, alignement
  temporel automatique et polarité, calculés au chargement (coût live = une IR) ; identifiants
  `c…`, conseillés en premier par `models` / `-i auto`.

### Ajouté (#14)
- Identifiants courts et stables pour chaque modèle (`m…`) et IR (`i…`), affichés par `models`,
  acceptés par `-m` / `-i`.
- `models` indique si chaque modèle a besoin d'une IR et conseille la meilleure (même marque,
  baffle basse) ; `-i auto` ; `run` avertit quand une capture sans baffle est jouée sans IR ;
  `default_ir` dans la config.

### Modifié (#14)
- Recherche par mots-clés : mots entiers prioritaires, et version « de base » préférée
  (`angel` -> « Angel », `angel v2` -> « Angel_v2 ») ; les candidats ambigus affichent leur
  identifiant.

### Ajouté
- `namfoop setup` : assistant plein écran (backend, carte, vumètres par entrée, bip par sortie,
  test automatique des buffers), qui enregistre `[audio]` et `[routing]` dans la config.
- Cartes multicanaux : namfoop ouvre tous les canaux de la carte (ex. Soundcraft MTK 14/12).
- Routage : `--bass-in`, `--mic-in`, `--route N=processed|di|mic|mute` (remplace `--out2`).
- Backend `sim` : carte simulée (basse synthétique) pour tester sans matériel.
- Options globales `--config` / `--no-config` (communes à toutes les commandes).
- Harnais : cycle issue -> branche -> PR -> merge (`scripts/feature.sh`), hooks git qui refusent
  commits et pushs directs sur `main`, règles de PR vérifiées en CI, modèles d'issue et de PR.
- CI plus rapide : plus de double exécution par PR, jobs lint et check en parallèle, profil `ci`
  (tests et binaire compilés une seule fois, sans LTO), cache sccache pour Rust et le C++ NAM.

### Modifié
- Plus besoin de `pw-jack` : namfoop trouve la libjack de PipeWire et se relance avec si besoin.
- Les réglages audio (`--backend`, `--device`, `--buffer`...) viennent de la config s'ils ne sont
  pas donnés.
- `calibrate --channel` prend par défaut l'entrée basse du routage ; `calibrate --config` devient
  l'option globale `--config`.

### Supprimé
- `run --out2` (remplacé par `--route 2=di|mute|processed`).

## 0.1.0 — 2026-11-08

Premier prototype.

### Ajouté
- Chaîne ampli : trim d'entrée -> modèle NAM -> IR -> niveau -> master -> limiteur.
- Modèles NAM via NeuralAmpModelerCore v0.6.0 (WaveNet, LSTM, A2, format 0.7 `SlimmableContainer`,
  choix de taille `--size`).
- IR de baffle : convolution sans latence, rééchantillonnage automatique, troncature (`--ir-ms`),
  normalisation sur 80 Hz – 5 kHz.
- Backends JACK (PipeWire) et ALSA direct (flux liés, priorité temps réel via rtkit, libération
  automatique de la carte tenue par PipeWire).
- `run` : ligne d'état (crêtes, limiteur, charge DSP, xruns, écrêtages), sortie 2 `same|di|mute`.
- `calibrate` : mesure du niveau de la basse, trim d'entrée, volume « match » aligné sur le DI.
- `render`, `bench`, `models`, `ports`, `latency`.
- Recherche des modèles et IR par mots-clés dans `library/`.
- Configuration `~/.config/namfoop/config.toml`.
- Harnais : `scripts/check.sh` (format, clippy, tests dont absence d'allocation temps réel et
  indépendance au bloc, cohérence de la documentation, rendu de la bibliothèque),
  `scripts/test-backends.sh` (tests matériels sur les deux backends).

// API C minimale autour de NeuralAmpModelerCore, consommée par Rust (crates/nam-sys/src/lib.rs).
// Aucune exception ne traverse cette frontière.
#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NamModel NamModel;

// Charge un .nam (ou un .wav, modèle linéaire). Renvoie NULL en cas d'erreur, message dans err.
// Non temps réel.
NamModel* nam_load(const char* path, char* err, int err_len);

// Prépare le modèle (allocations + prewarm). Non temps réel. Renvoie 0 si OK.
int nam_reset(NamModel* m, double sample_rate, int max_block, char* err, int err_len);

// Traite n frames mono (n <= max_block). Temps réel : pas d'allocation.
void nam_process(NamModel* m, const float* in, float* out, int n);

double nam_expected_sample_rate(const NamModel* m);
// Loudness en dB (sortie pour une entrée « typique »), *has = 0 si absente.
double nam_loudness(const NamModel* m, int* has);

// Modèles « slimmable » : 0.0 (léger) .. 1.0 (complet). Non temps réel.
// Renvoie 0 si appliqué, -1 si le modèle n'est pas slimmable.
int nam_set_slimmable_size(NamModel* m, double size);
// Écrit jusqu'à `cap` breakpoints, renvoie leur nombre total (-1 si non slimmable).
int nam_slimmable_breakpoints(const NamModel* m, double* out, int cap);

void nam_free(NamModel* m);

#ifdef __cplusplus
}
#endif

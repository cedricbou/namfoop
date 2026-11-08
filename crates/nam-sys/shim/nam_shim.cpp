#include "nam_shim.h"

#include <cstdio>
#include <cstring>
#include <exception>
#include <filesystem>
#include <memory>
#include <vector>

#include "NAM/dsp.h"
#include "NAM/get_dsp.h"
#include "NAM/slimmable.h"

struct NamModel
{
  std::unique_ptr<nam::DSP> dsp;
  // NAM_SAMPLE est double par défaut : tampons de conversion préalloués dans nam_reset.
  std::vector<NAM_SAMPLE> in;
  std::vector<NAM_SAMPLE> out;
};

static void set_err(char* err, int len, const char* msg)
{
  if (err && len > 0)
  {
    std::snprintf(err, (size_t)len, "%s", msg);
  }
}

extern "C" NamModel* nam_load(const char* path, char* err, int err_len)
{
  try
  {
    nam::DspLoadOptions opts;
    opts.prewarm = false; // le prewarm se fait dans nam_reset, au bon sample rate
    auto dsp = nam::get_dsp(std::filesystem::path(path), opts);
    if (!dsp)
    {
      set_err(err, err_len, "get_dsp a renvoyé null");
      return nullptr;
    }
    auto* m = new NamModel();
    m->dsp = std::move(dsp);
    return m;
  }
  catch (const std::exception& e)
  {
    set_err(err, err_len, e.what());
  }
  catch (...)
  {
    set_err(err, err_len, "exception inconnue");
  }
  return nullptr;
}

extern "C" int nam_reset(NamModel* m, double sample_rate, int max_block, char* err, int err_len)
{
  try
  {
    m->in.assign((size_t)max_block, 0);
    m->out.assign((size_t)max_block, 0);
    m->dsp->Reset(sample_rate, max_block); // prewarm inclus
    return 0;
  }
  catch (const std::exception& e)
  {
    set_err(err, err_len, e.what());
  }
  catch (...)
  {
    set_err(err, err_len, "exception inconnue");
  }
  return -1;
}

extern "C" void nam_process(NamModel* m, const float* in, float* out, int n)
{
  for (int i = 0; i < n; i++)
    m->in[i] = (NAM_SAMPLE)in[i];
  NAM_SAMPLE* ip = m->in.data();
  NAM_SAMPLE* op = m->out.data();
  m->dsp->process(&ip, &op, n);
  for (int i = 0; i < n; i++)
    out[i] = (float)m->out[i];
}

extern "C" double nam_expected_sample_rate(const NamModel* m)
{
  return m->dsp->GetExpectedSampleRate();
}

extern "C" double nam_loudness(const NamModel* m, int* has)
{
  *has = m->dsp->HasLoudness() ? 1 : 0;
  return *has ? m->dsp->GetLoudness() : 0.0;
}

extern "C" int nam_set_slimmable_size(NamModel* m, double size)
{
  auto* s = dynamic_cast<nam::SlimmableModel*>(m->dsp.get());
  if (!s)
    return -1;
  try
  {
    s->SetSlimmableSize(size);
    return 0;
  }
  catch (...)
  {
    return -1;
  }
}

extern "C" int nam_slimmable_breakpoints(const NamModel* m, double* out, int cap)
{
  auto* s = dynamic_cast<const nam::SlimmableModel*>(m->dsp.get());
  if (!s)
    return -1;
  auto bp = s->GetSlimmableSizeBreakpoints();
  for (int i = 0; i < (int)bp.size() && i < cap; i++)
    out[i] = bp[(size_t)i];
  return (int)bp.size();
}

extern "C" void nam_free(NamModel* m)
{
  delete m;
}

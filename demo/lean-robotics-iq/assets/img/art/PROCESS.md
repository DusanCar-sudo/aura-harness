# Lean Robotics IQ — editorial art set

Nine plates for the Lean Robotics IQ site, generated on the local SDXS-512
DreamShaper server (`http://127.0.0.1:8190`) and post-processed into a
two-ink screenprint / archival-plate look. No external image assets, no text
rendered in any plate.

Everything here is produced by `_build/*.py` against the running service; the
systemd unit was only ever called over HTTP.

## 1. Model + generation

- Model: `sdxs.safetensors` (SDXS-512 DreamShaper), stable-diffusion.cpp server.
- Native quality setting: **512-class canvas, 1 step, CFG 1.0, Euler** — the
  model is deliberately small/fast and produces composed-but-imperfect art;
  that grain of error is kept, not corrected.
- Generation sizes match the delivery aspect ratio so the final plates only
  need a Lanczos upscale/crop, never a reframe:
  hero 512x640, loop/gate/dataset/verify 576x384, lang 512x512, paper 576x368,
  og 640x336.

**Important server detail:** the OpenAI-compatible route
`POST /v1/images/generations` is prompt-keyed cached on this server — it
ignores `seed` and `steps` for a repeated prompt (identical MD5 for different
seeds) and only returns a fresh variant when `n>1`. All variants were therefore
generated through the native route, which honours seed/steps and returns the
sampling info used in `manifest.json`:

```bash
curl -s -X POST http://127.0.0.1:8190/sdapi/v1/txt2img \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"risograph poster, two ink colours orange and black on beige paper,
       small robot standing in an immense archive of index card drawers, ...
       flat shapes, halftone, high contrast, minimal composition, generous negative space",
       "negative_prompt":"text, letters, words, typography, numbers, watermark, signature,
       human face, portrait, photograph, 3d render, gradient, harsh shadows",
       "width":512,"height":640,"steps":1,"cfg_scale":1.0,
       "sampler_name":"euler","seed":5001,"batch_size":1,"n_iter":1}'
```

Batch driver: `python3 _build/gen.py _build/jobs_all.json` (34 candidates:
2 prompt wordings x 2 seeds per concept, plus extra hero seeds; ~5-9 s/image).
Raw model output is untouched in `raw/<concept>/` (46 concept variants plus 8 style tests) (see the manifest's
`alternatives` field for the runner-up of each plate).

Prompt shape used for the set (the flat riso phrasing is what makes the
1-step model produce poster-like shapes instead of photographs):

> risograph poster, two ink colours orange and black on beige paper, **{subject}**,
> flat shapes, halftone, high contrast, minimal composition, generous negative space

Subject one-liners per plate are in `manifest.json` (`concept` + `prompt`).

## 2. Post-processing (the consistent look)

`_build/treat.py`, function `printify()`:

1. open raw PNG, `ImageOps.fit` cover-crop to the exact delivery size (Lanczos,
   2-2.2x upscale) — composition preserved, no reframing;
2. grayscale, `GaussianBlur(soften≈1.0-1.2)` to soften photoreal micro-detail;
3. `MedianFilter(3)` to flatten detail into print-like areas;
4. `autocontrast(cutoff=1)` + an S-curve tone LUT (`contrast`/`gamma`/`lift`
   tuned per plate: the bone plates get high-key tone to keep pale paper,
   gate/og stay hotter for orange);
5. colour map (duotone LUT):
   - **bone plates** (hero-atlas, loop, dataset, verify, lang, paper):
     ink `#0D0F0E` -> bone `#EFEAE0` (pale paper, deep blacks);
   - **signal-orange plates** (gate, og): ink `#0D0F0E` -> `#E8502A`;
6. palette quantise to 8-14 discrete print tones with Floyd-Steinberg dither =
   visible halftone dots; `levels` controls how coarse the screen is;
7. film grain: gaussian noise, monochrome, weighted toward midtones (subtle,
   amplitude 3-4/255);
8. JPEG **quality 80-82, progressive, 4:2:0**, metadata stripped, <= 250 KB
   (build loop steps quality down 2 points only if a plate misses the cap).

Exact per-plate parameters are recorded in `manifest.json` under `post`.

```bash
python3 _build/build_finals.py _build/finals.json   # renders all plates + manifest
python3 _build/grain.py grain.jpg                  # synthesised paper-grain tile
```

`grain.jpg` is not model-generated: seamless FFT band-limited bone-paper fibre
plus a fine weave at 512x512 (subtle, ~16 KB) so it can tile behind type.

## 3. Delivered files

| file | dims | bytes | source raw | treatment |
|---|---|---|---|---|
| `hero-atlas.jpg` | 1024x1280 | 251,878 | `raw/hero/p1_s222.png` | bone duotone |
| `loop.jpg` | 1200x800 | 153,379 | `raw/loop/loop_v0s5051.png` | bone duotone, high-key |
| `gate.jpg` | 1200x800 | 109,417 | `raw/gate/gate_v0s5001.png` | signal-orange duotone |
| `dataset.jpg` | 1200x800 | 192,252 | `raw/dataset/dataset_v0s5051.png` | bone duotone |
| `verify.jpg` | 1200x800 | 152,523 | `raw/verify/verify_v0s5001.png` | bone duotone |
| `lang.jpg` | 1000x1000 | 127,926 | `raw/lang/lang_v1s5112.png` | bone duotone |
| `paper.jpg` | 1400x900 | 183,816 | `raw/paper/paper_v0s5051.png` | bone duotone, high-key |
| `og.jpg` | 1200x630 | 107,245 | `raw/hero/p1_s222.png` | signal-orange duotone crop |
| `grain.jpg` | 512x512 | 16,064 | synthesised | bone paper fibre tile |
| `contact-sheet.jpg` | 1554x1620 | 235,099 | all plates | review flat-lay, labelled |

`manifest.json` carries filename, concept, prompt, seed, native generation
params, exact treatment params, source raw path, alternatives and notes.

## 4. Warts / alternatives (also in manifest.json)

- `dataset.jpg` — cabinet reads more mid-century sideboard than specimen archive
  at a glance; cleanest-but-least-surreal plate.
- `loop.jpg` — keeps the source's plate border; one malformed tooth cluster on
  the lower-right gear (1-step artefact, reads as print damage).
- `lang.jpg` — the carved "symbols" are abstract glyph shapes, not a legible
  alphabet; the reaching limb exits the frame. (The `v0` tower variants carry
  real letter-like marks and were rejected for the no-text rule.)
- `verify.jpg` — tiny illegible signature scribble in the bottom-right margin of
  the source render; below display-size legibility, deliberately not retouched.
- `hero-atlas.jpg` — 246 KB is the largest plate, right under the 250 KB cap
  (bone-paper grain compresses less); quality 80 rather than 82.

Runners-up for every plate live in `raw/<concept>/` next to the chosen render,
so any plate can be swapped by editing `_build/finals.json` and re-running
`build_finals.py`.

## 5. Verification

```bash
cd assets/img/art
identify -format '%f %wx%h %b\n' hero-atlas.jpg loop.jpg gate.jpg dataset.jpg \
  verify.jpg lang.jpg paper.jpg og.jpg grain.jpg contact-sheet.jpg
# every plate <= 250 KB, exact target dimensions, valid JPEG
```

Each delivered plate and the contact sheet were re-opened and eyeballed after
the final render pass; no plate contains rendered type, human faces or watermarks.

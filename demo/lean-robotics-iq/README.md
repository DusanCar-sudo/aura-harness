# Lean Robotics IQ — local site

Static website for **Lean Robotics IQ**, the robotics-dataset localization line of Lean Progress IQ
(Praktess Research / Aura Code Project). The site is a new venture build in this repository; the
previous Lean Progress IQ site is archived untouched at
[`_archive/lean-progress-iq-site_2026-10-22.zip`](_archive/lean-progress-iq-site_2026-10-22.zip).

Live local preview (while the preview server is running):

```
http://127.0.0.1:8191/
```

Local image generation used for the plates is the on-device service at
`http://127.0.0.1:8190` (SDXS-512 DreamShaper, 1-step).

## What this site is

A research-led company site for localizing the instruction and annotation layer of robot-learning
datasets: instructions, subtasks, captions, success criteria, failure notes, dialogue and eval text.
The argument, the method and the evidence are explicit, and the site states its limits:

- The method is an **adaptation** of the author's preprint
  *Recursive Learning and the Archimedes Principle* (2025, preprint — not peer reviewed). The
  preprint studies autonomous coding agents; it contains no robot data or translation experiment.
  This is stated on [`method.html`](method.html).
- The audit quoted on [`index.html`](index.html#evidence) is the preprint's coding-agent deployment
  audit: 1,225 episodes, 153 instrumented decisions, 0 small-tier attempts, 143/143 direct-to-large.
- Dataset figures are verified against official dataset cards/papers; sources, confidence labels and
  the "do not claim" list live in the claims dossier: [`content/claims-dossier.md`](content/claims-dossier.md)
  (also served at [`assets/docs/claims-dossier.md`](assets/docs/claims-dossier.md)).
- All imagery is generated locally by a small model that is good at mistakes. No cloud images, no
  stock, no third-party fonts or scripts.

## Build and run

The site is plain static HTML/CSS/JS. Source pages live in `src/`; `build.py` assembles them into
the repository root. No dependencies beyond Python 3 and the standard library.

```bash
# build the four pages into the repo root
python3 build.py

# validate without touching root output
python3 build.py --check

# serve locally (http://127.0.0.1:8191)
python3 -m http.server 8191 --bind 127.0.0.1 --directory .
```

Generated files at the root (`index.html`, `method.html`, `datasets.html`, `privacy.html`,
`404.html`) are committed so the site can be hosted by any static host without a build step.

## Structure

```
src/
  layout.html             document shell, meta, includes
  partials/               nav, footer, scripts
  pages/                  index, method, datasets, privacy, 404
assets/
  css/site.css            design system (no framework)
  js/site.js              nav, reveals, tabs, counters, loop diagram, bench calculators
  fonts/                  self-hosted woff2 latin subsets (OFL)
  img/art/                generated plates (finals), manifest.json, PROCESS.md
  img/favicon.svg         mark
  docs/                   preprint PDF, claims dossier
build.py                  stdlib page builder (front matter + include + token substitution)
tools/                    reproducible social-card renderer (og-card.html + make-og.js)
content/                  research notes (claims dossier; not page copy)
_archive/                 zip of the previous Lean Progress IQ site
.work/                    local QA artifacts (ignored by git)
```

## Design system

Editorial-technical: bone paper `#EFEAE0`, ink `#0D0F0E`, signal orange `#E8502A`, phosphor lime
`#C8F04B`. Type: Archivo (display), Barlow (body), Clash Display (accent/emphasis), JetBrains Mono
(labels). All fonts are self-hosted: Archivo, Barlow and JetBrains Mono under the SIL Open Font
License; Clash Display by Indian Type Foundry under the Fontshare free font licence. Motion is
CSS/IntersectionObserver only and respects `prefers-reduced-motion`.

Interactive pieces:

- the recursion loop (`R = I ∘ D`) with stage detail on `index.html#method` and `method.html`;
- the language line tabs with action-spine annotations (`index.html#demo`);
- the **break-even bench** (`method.html#bench`) implementing `E[C_gate] = c_S + (1−q)c_L` and
  `q > c_S/c_L` live;
- the **verifier envelope calculator** (`method.html#envelope`) implementing
  `P[error] ≤ (1−p)α + (1−p_L)`.

## Image provenance

Plates are generated on the local machine (`127.0.0.1:8190`, SDXS-512 DreamShaper distilled, 1 step,
512 px source) and post-processed to duotone screenprint, grain and the brand palette. Prompts, seeds,
treatments and exact commands are recorded in [`assets/img/art/manifest.json`](assets/img/art/manifest.json)
and [`assets/img/art/PROCESS.md`](assets/img/art/PROCESS.md). Raw candidates are kept under
`assets/img/art/raw/` for reproducibility; the site references only the final files. Two
post-processing steps added after delivery (a crop of the side bars in `lang.jpg`, and the branded
`og-card.jpg` composed in a headless browser from the hero plate) are documented in
[`assets/img/art/POSTPROCESS-LEAD.md`](assets/img/art/POSTPROCESS-LEAD.md).

## Before publishing — confirm these placeholders

The build is complete and servable, but a few business facts are placeholders that only the founder
can confirm:

1. **Domain** used in the copy (`leanroboticsiq.com` is a placeholder; meta/`vercel.json` carry no
   domain). When the domain is fixed, make the `og:image` / `twitter:image` URLs absolute.
2. **Language lines** and their status labels on `index.html#languages` (SR/HR/BS/VI "pilot open",
   DE/ES/FR/JA "opening").
3. **Pilot terms** on `index.html#contact` (2,000 strings, ~10 working days) and the engagement
   descriptions.
4. **Email** `aura@leanproiq.com` (from the ORCID record) and the contact person.
5. The claims in the **"prior art, acknowledged"** callout are source-backed; re-check the dossier
   before quoting them publicly.

Deployment: `vercel.json` contains clean URLs, security headers and cache rules only. No analytics,
no trackers, no cookies. `privacy.html` describes what the software actually does.

## Provenance of the underlying research

- Preprint PDF (served): [`assets/docs/Milosavljevic_2025_Recursive-Learning-and-the-Archimedes-Principle.pdf`](assets/docs/Milosavljevic_2025_Recursive-Learning-and-the-Archimedes-Principle.pdf)
- ORCID: <https://orcid.org/0009-0006-4706-0467>
- Google Scholar: <https://scholar.google.com/citations?user=Gbp13nYAAAAJ&hl=en>

Corrections and source updates go through the claims dossier first, then the pages. The site's rule:
publish corrections rather than quietly editing claims.

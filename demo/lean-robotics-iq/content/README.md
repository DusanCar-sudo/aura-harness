# `content/` — research provenance for Lean Robotics IQ

This folder holds the **evidence layer** behind the site's claims. It is not page copy and it is not marketing copy. If a sentence on the site makes a factual claim, the answer to "where does that come from?" should be findable here in under a minute.

## Inventory

| File | What it is | Status |
|---|---|---|
| `claims-dossier.md` | Main deliverable: dataset language facts (§A), prior art (§B), market framing + safe phrasings (§C), format facts and localized-deliverable design (§D), exact preprint facts with section refs and glosses (§E), do-not-claim list + rewrites (§F), source table (§G), open questions (§H), verified multilingual examples (App. A), safe copy blocks (App. B). | Current, verified 2026-10-22 |
| `README.md` | This file: confidence legend, refresh commands, rules for adding claims, link-check procedure. | Current |

**Not created:** `sample-translations.md`. The brief allowed it only if we could source real multilingual robot instructions from an *official* dataset card. No official dataset card with official multilingual instructions was found. The community examples we did verify (Greek LIBERO pairs, one Arabic PushT instruction) are logged in `claims-dossier.md` Appendix A with provenance caveats and a "do not reuse without license check" warning.

## Confidence legend (use these labels in every future addition)

| Label | Definition | What it is allowed to support on the site |
|---|---|---|
| **[V]** | Verified from a primary/official source: the exact number or wording appears on the source page (or in the paper PDF/LaTeX), and the URL + access date are recorded in the dossier. | A direct statement, with a link. |
| **[I]** | Inferred from official material: the source strongly implies it but does not state it (the classic case: all published instruction examples are English, but the card never says "English"). | Only with a qualifier ("documented in English", "in all published examples", "we found no official multilingual release"). |
| **[U]** | Unverified / unknown: source unreachable, ambiguous, or no source found. | Nothing. Appears only in §H open questions or as an explicit "unverified" in copy. |
| **[X]** | Do not use: contradicted or overclaiming. | Nothing. |

Also record a **snapshot date** for anything that can change (dataset indexes, row counts, Hugging Face cards). `2026-10-22` is the snapshot date for this dossier.

## Refresh procedure

Rule: never edit a fact without re-fetching its source, and update both the `Last verified` line at the top of `claims-dossier.md` and the access date of the rows you touched in §G.

### 1. Founder preprint identity and status (do first — it gates §E and the footer)

```bash
# ORCID record (authoritative for the work; ORCID lists type "preprint", date, URL, no DOI)
curl -sS -H "Accept: application/json" \
  "https://pub.orcid.org/v3.0/0009-0006-4706-0467/record" | python3 -m json.tool | head -80

# Author's abstract page — check it still says "Preprint · Not peer reviewed"
curl -sSL "https://elub-portfolio.vercel.app/abstract" | grep -o -i -E ".{0,80}(preprint|peer.reviewed).{0,80}"

# Local paper version drift check (current version is /mnt/bigdata/aura/projects/orcid-paper/paper.pdf)
md5sum "/mnt/bigdata/aura/projects/orcid-paper/paper.pdf" \
       "/mnt/bigdata/aura/projects/leanproiq-site/.work/recursive-learning.pdf"
diff <(pdftotext "/mnt/bigdata/aura/projects/orcid-paper/paper.pdf" -) \
     <(pdftotext "/mnt/bigdata/aura/projects/leanproiq-site/.work/recursive-learning.pdf" -) | head -20
```

What to check: work type (`preprint`), publication date, absence of external IDs, presence of the "not peer reviewed" line, and whether the PDFs differ by more than the cover date. Also inspect `activities-summary.educations` and `activities-summary.qualifications`: these are author-self-asserted; the Google entry is a course badge, **not** employment (see dossier §E.0.1). Never turn it into "ex-Google" copy. If a DOI or venue appears, **stop** and ask the founder before any copy claims peer review — that would be a material change to §F.

Expected failure: Google Scholar (`https://scholar.google.com/citations?user=Gbp13nYAAAAJ&hl=en`) returns HTTP 429 to automated fetches. That is why no citation metrics are claimed. Do not "verify" metrics via a scraper.

### 2. Open X-Embodiment index statistics (§A.1, §C.2)

```bash
curl -sL "https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/export?format=csv&gid=0" -o oxe.csv
python3 - <<'PY'
import csv, collections
rows=list(csv.reader(open('oxe.csv', newline='', encoding='utf-8', errors='ignore')))
data=[r for r in rows[15:] if len(r)>11 and r[1].strip()]
print('datasets:', len(data))
print('language-annotation values:', collections.Counter(r[11].strip() or '(blank)' for r in data))
print('sum episodes:', sum(int(r[3].replace(',','')) for r in data if r[3].replace(',','').isdigit()))
PY
```

Record: dataset count, episode total, the Language Annotations distribution, and the snapshot date. These numbers grow; the dossier's "72 datasets / 2,419,193 episodes / 26 Templated / 15 Natural / 26 None" is a 2026-10-22 snapshot.

Also refresh if needed: [OXE project site](https://robotics-transformer-x.github.io/) (1M+, 60 datasets, 34 labs, 22 embodiments, 527 skills / 160,266 tasks), [paper arXiv:2310.08864](https://arxiv.org/abs/2310.08864), [GitHub](https://github.com/google-deepmind/open_x_embodiment).

### 3. Dataset primary facts (§A.2–A.7)

| Dataset | Refresh source |
|---|---|
| DROID | https://droid-dataset.github.io/ (scale, updates) · https://arxiv.org/abs/2403.12945 (annotation method) · https://huggingface.co/KarlP/droid |
| RT-1 | https://robotics-transformer1.github.io/ (130k/700+/13/17) · https://arxiv.org/abs/2212.06817 |
| BridgeData V2 | https://rail-berkeley.github.io/bridgedata/ (60,096 / 24 / 13) · https://arxiv.org/abs/2308.12952 (post-hoc annotations) |
| ALOHA / ACT | https://tonyzhaozh.github.io/aloha/ (50 demos, 4 tasks) · https://arxiv.org/abs/2304.13705 · https://github.com/tonyzhaozh/act |
| LIBERO | https://arxiv.org/abs/2306.03310 (130 tasks, 50 demos/task) · https://github.com/Lifelong-Robot-Learning/LIBERO · raw loader: https://raw.githubusercontent.com/Lifelong-Robot-Learning/LIBERO/master/libero/libero/utils/dataset_utils.py |
| LeRobot | https://huggingface.co/docs/lerobot/lerobot-dataset-v3 · raw language docs: https://raw.githubusercontent.com/huggingface/lerobot/main/docs/source/language_and_recipes.mdx · https://huggingface.co/docs/lerobot/annotation_pipeline · https://arxiv.org/abs/2602.22818 |
| AgiBot World | https://arxiv.org/abs/2503.06669 |
| RoboMIND | https://arxiv.org/abs/2412.13877 |

When a card is gated (HTTP 401/403), **do not** guess the content. Mark [U] and move on.

### 4. Prior art (§B) — re-check before any "first/gap" copy

- RxR: https://aclanthology.org/2020.emnlp-main.356/
- ReLI: https://arxiv.org/abs/2505.01862
- ACL 2026 multilingual LIBERO: https://aclanthology.org/2026.acl-long.2066/ · https://arxiv.org/abs/2606.11906
- Beyond English: https://arxiv.org/abs/2606.15714
- Greek language transfer: https://arxiv.org/abs/2609.07470 · https://huggingface.co/papers/2609.07470
- Arabic PushT: https://huggingface.co/datasets/hamzabouajila/ar-pusht-image
- Greek LIBERO policy: https://huggingface.co/ayoubkirouane/Sophea-Nano-Policy-LIBERO-Greek-v1
- ALLMAN German VLA: PDF unreachable at research time — recheck only if it becomes relevant; until then it stays [U].

This area moves monthly. Any sentence of the form "no one has…" must be re-derived (date-stamped "we found no…") right before publication.

### 5. Formats (§D)

- RLDS: https://github.com/google-research/rlds (Dataset Format section)
- LeRobot v3: https://huggingface.co/docs/lerobot/lerobot-dataset-v3 · language columns: raw URL above
- LIBERO HDF5: loader lines in `dataset_utils.py` (the `problem_info` → `language_instruction` path)
- DROID RLDS quickstart: https://droid-dataset.github.io/ (`step["language_instruction"]`)
- ALOHA/ACT: HDF5 episodes; no language input in ACT (do not claim otherwise)

## Quarterly / pre-launch checklist

- [ ] Re-fetch §G sources; update access dates and `Last verified`.
- [ ] Recompute OXE index snapshot; adjust the three counts in §A.1, §C.1, §C.2.
- [ ] Re-run the prior-art sweep for 2025–2026 multilingual robot papers; if a dataset-side localization product appears, revise §B.3 differentiators and §C immediately.
- [ ] Check that no page copy uses a banned term from §F.1 (search the HTML for "peer-reviewed", "proven", "%", "first", "100%", "trusted by").
- [ ] Confirm the footer citation still matches §E.0 (no DOI added).
- [ ] Run the link check below; triage failures.
- [ ] If any source was retracted/edited, record the change here and in §H.

## Link check

Run from the repo root (`/mnt/bigdata/aura/projects/leanproiq-site`), because paths below are relative.

```bash
python3 - <<'PY'
import re, urllib.request
md = open('content/claims-dossier.md', encoding='utf-8').read()
urls = sorted(set(re.findall(r'\]\((https?://[^)\s]+)\)', md)))
bad = []
for u in urls:
    try:
        req = urllib.request.Request(u, headers={'User-Agent':'Mozilla/5.0 (link-check; contact aura@leanproiq.com)'})
        code = urllib.request.urlopen(req, timeout=25).status
    except Exception as e:
        code, bad = 'ERR', bad + [(u, str(e))]
    print(code, u)
print('FAILURES:', bad)
PY
```

Expected non-2xx: Google Scholar (429), the ALLMAN PDF (unreachable), Google Sheets edit URL (may return HTML/login — the CSV export URL is the one that matters), and any gated HF repo. Do **not** silently remove a failing link: either replace it with a stable one, mark the claim [U], or document the failure in §H.

## How to add a claim (template)

1. Find the **primary** source. Prefer the paper/project site/dataset card over blogs, news, aggregators, or other agents' summaries.
2. Capture the exact wording or number, the URL, and the access date.
3. Assign [V]/[I]/[U]/[X] using the legend above.
4. Add a row to §G if it is a new source, and cite the section where the claim is used.
5. If it is a *strong* claim (a number, a comparison, a gap/first claim), add it to §F.2 (risky → safe) or §F.3 (safe today) as appropriate.
6. If two sources disagree (e.g. paper's 60 datasets vs index's 72), keep both with their counting basis and date — never average or silently pick one.

## Copy-editor rules (short version)

- Link every number. A number without a link is a placeholder, not a fact.
- Date-stamp absence claims: "we found no official multilingual release (checked 2026-10-22)".
- Never write "100%", "all", "every", "first", "only", "proven", "peer-reviewed", "DOI", "trusted by", "% improvement".
- Any sentence containing "English-only" should be replaced with the safe phrasing in §C.4.
- The preprint's audit numbers (1,225 episodes, 143 tasks, 0 attempts, 1.049B tokens, 68.2%, 93.5%, 8.6 s) are **coding-agent deployment facts**. If used at all, label them as such and link §E.0. Never present them as robotics or translation results.
- The 20% cost figure is a hypothesis, not a result (§E.3). The 30–50% figure is an external ACL 2026 finding about *degradation*, not our performance (§B.3).

## Known gaps

See `claims-dossier.md` §H. The most important: DROID/RoboMIND/AgiBot instruction *language* is inferred, not stated; no official multilingual dataset release exists to inspect; Scholar metrics unavailable; no market-size research was done here.

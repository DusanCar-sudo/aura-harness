# Lean Robotics IQ — Source-Verified Claims Dossier

**Status:** research note for site copy. Not page copy. Write scope: `content/` only.
**Last verified:** 2026-10-22 (Asia/Saigon). Every source URL below was fetched on that date unless marked otherwise.
**Audience:** whoever writes or approves copy for the new Lean Robotics IQ site, plus the founder.

**Confidence labels (used everywhere in this file)**

| Label | Meaning |
|---|---|
| **[V]** | Verified from a primary/official source: the number or wording was read on the source page (or in the paper PDF/LaTeX) and the exact URL is given. |
| **[I]** | Inferred from official material: the source strongly implies it (e.g. every documented example is an English string) but does not state it explicitly. Usable only with qualifying language on the site. |
| **[U]** | Unverified / unknown: source unreachable, ambiguous, or no source found. Do **not** put on the site as fact. |
| **[X]** | Do not use: contradicted, or a known overclaim. |

**Rules this file follows** (same rules the site should follow):

1. Every factual statement carries a real URL. If a number has no link here, it is a recommendation, not evidence.
2. "We found no X" is written with the date and the searches done. It always means *not found*, never *does not exist*.
3. The founder's paper is cited from the author's own abstract page + ORCID record. It has **no DOI, no journal, no conference acceptance, and no peer review** — see §E.0.
4. No invented statistics, customers, coverage figures, or "first/only" claims. Industry rules of thumb (e.g. "most of the field is English") are flagged **[I]** and given a safe phrasing in §C and §F.
5. Where the paper and a fact table disagree on counting basis, the dossier says so instead of mixing numbers.

---

## 0. TL;DR — the ten things that matter for copy

1. **The instruction layer really is documented in English.** Seven of the most-used open robot corpora (Open X-Embodiment/RT-X, DROID, RT-1, BridgeData V2, ALOHA/ACT, LIBERO, LeRobot) all document English-language task strings. The OXE dataset index records annotation *style* — 26 "Templated", 15 "Natural", 26 "None" out of 72 datasets — and has **no language/locale column at all** [V, §A.1]. "Overwhelmingly English" is defensible as a dated inference [I]; "100% English" is not.
2. **Multilingual robot-instruction research is real and active in 2026.** LIBERO has been translated into ten languages for evaluation (success drops 30–50%) [V, §B]; two 2026 papers map the multilingual gap in VLA models [V]; a Greek case study shows bilingual training recovers only ~2/5 of English performance and overfits the translator's phrasing [V]. A site that claims "nobody has done this" will be embarrassed in one search.
3. **The real gap is dataset-side, provenance-tracked localization** — not "translation exists". What we found is benchmark-side evaluation translations (LIBERO→10 languages), model-side language adaptation (Greek), one community dataset translation (Arabic PushT), and navigation-only multilingual data (RxR). We found no product that localizes a robot dataset *in its native format with per-episode provenance and QA* [I — absence of evidence, dated].
4. **The founder's preprint is a coding-agent cost study, not robotics research.** It contains no robot data, no translation experiment, no localization evaluation [V, §E]. Applying its ideas to translation QA is an *adaptation* and must be described that way. This is the single biggest overclaim risk on the site.
5. **The preprint is not peer reviewed.** The author's own abstract page says "Preprint · Not peer reviewed"; the PDF says "Manuscript status: Preprint. Not peer reviewed. Contains a pre-registered protocol (Section 8) that has not yet been executed." [V, §E.0]
6. **The audit is a negative result.** 1,225 episodes, 153 instrumented alternation decisions, **0** small-tier attempts, 143/143 direct-large task rows; the gate never fired, and the paper predicts why [V, §E.1]. Safe to quote *if labeled* as the founder's coding-agent deployment. Never as a robotics result.
7. **Do not claim savings.** The 20% cost reduction is a *pre-registered hypothesis* (§8.3, H8.1) that was not run; the audit's own conclusion is that the mechanism was "empirically, untested" [V, §E.3–E.4].
8. **Translation quality is the technical crux, which is good news for a QA-led service.** The Greek study found the policy "overfits the translator's phrasing"; training on seven phrasings per task "approximately halves this penalty" [V, §B.4]. Paraphrase diversity + independent verification are sellable, evidence-backed differentiators.
9. **Three strongest differentiators** (details §B.3): (a) format-native, provenance-preserving dataset localization (episode/task IDs preserved, manifest published); (b) validated paraphrase sets, not single literal translations, because the 2026 evidence says phrasing diversity is what transfers; (c) a published QA protocol (back-translation + native-speaker adjudication + wrong-instruction controls + coverage/failure reporting).
10. **Hard bans for the site:** "peer-reviewed", "published study/paper" without "preprint", "proven", "validated on robots", "% improvement", "first/only", "100% of datasets are English", "supports N languages", customer logos, "state of the art". Full list and safe rewrites in §F.

---

## A. Robot-learning datasets and the language of their instructions

**Summary table.** All seven are documented in English; none of the official artifacts we checked ships a locale/language field or an official translation.

| Dataset | What it is | Scale (as documented) | Language annotations | Official translation? | Conf. |
|---|---|---|---|---|---|
| Open X-Embodiment / RT-X | Pooled cross-embodiment corpus + RT-1-X/RT-2-X models | 60 datasets, 22 robots, 21 institutions, 34 labs, 527 skills / 160,266 tasks, "1M+ real robot trajectories" | Per-step `language_instruction` in the RLDS builds; index column "Language Annotations" = Templated/Natural/None | No locale field; no translated release found | [V] scale, [I] English |
| DROID | In-the-wild Franka manipulation | 76k trajectories / 350 h; 564 scenes; 86 tasks; 50 collectors; 13 institutions; 12 months | 1–3 crowdsourced instructions per episode; 3 annotations for 95% of successful episodes (Dec 2024 update) | No | [V] scale/annotations, [I] English |
| RT-1 | Google Robot multi-task dataset + model | 130k+ episodes, 700+ tasks/instructions, 13 robots, 17 months | Templated instruction strings; model input is a task string | No | [V] scale, [I] English |
| BridgeData V2 | WidowX manipulation corpus | 60,096 trajectories (50,365 teleop + 9,731 scripted), 24 environments, 13 skills | Post-hoc crowdsourced task descriptions (not collected with task names) | No | [V] scale/annotations, [I] English |
| ALOHA / ACT | Low-cost bimanual hardware + ACT policy | 50 demos per task; site reports 4 tasks at 96/84/64/92% success | **Not language-conditioned**: policy input is images + joint positions (+ style z). Language only appears via wrappers (OXE/LeRobot) | No | [V] |
| LIBERO | Lifelong robot-learning benchmark | 130 tasks / 4 suites; 50 demos per task (paper); task `language` property per task | English task strings from templates; stored in HDF5 `problem_info` | No official multilingual release (research translations exist) | [V] scale, [I] English |
| LeRobot | Open-source robot-learning library + dataset format | Format v3 with `meta/tasks.jsonl`; optional `language_persistent` / `language_events` columns | "Most LeRobot datasets ship with a single `task` string per episode" | No translation/localization layer | [V] format, [I] English |

### A.1 Open X-Embodiment / RT-X

- **Official pages:** [project site](https://robotics-transformer-x.github.io/) · [paper arXiv:2310.08864](https://arxiv.org/abs/2310.08864) · [GitHub](https://github.com/google-deepmind/open_x_embodiment) · [dataset index / spreadsheet](https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/edit)
- **Scale [V]:** project site: "It contains 1M+ real robot trajectories spanning 22 robot embodiments"; "constructed by pooling 60 existing robot datasets from 34 robotic research labs around the world." Paper abstract: 22 robots, 21 institutions, "527 skills (160266 tasks)". Models: RT-1-X and RT-2-X; both take "a visual input and natural language instruction describing the task".
- **Live index numbers [V, accessed 2026-10-22]:** the spreadsheet now lists **72 datasets**, **2,419,193 episodes** and **8,964.94 GB**, i.e. it has grown past the paper's 60 datasets / 1M+ figures. Always date-stamp either number and never mix the two.
- **Language facts [V]:** the index has a column literally named **"Language Annotations"** with values `Templated` (26 datasets), `None` (26), `Natural` (15), plus five one-off descriptions (e.g. "Dataset does not contain language instruction annotation", "Multimodal (image + language) templated instructions"). There is **no column for language or locale**. English evidence on the project page is the instruction strings shown under the videos: "move red pepper to tray", "pick ice cream", "move red pepper to A".
- **Why it matters:** roughly a third of indexed datasets have no language annotations at all, and the schema records annotation style, not language. A localization company must handle "no language", "templated language", and "natural language" as separate jobs — this is a positioning gift that a simple "translate the instructions" pitch misses.
- **Confidence:** scale [V]; "annotations are English" = **[I]** (no dataset card we checked documents a non-English instruction set; none documents English explicitly either). Phrase as: "the OXE index has no language field, and every instruction example shown in its documentation is English."

### A.2 DROID

- **Official pages:** [project site](https://droid-dataset.github.io/) · [paper arXiv:2403.12945](https://arxiv.org/abs/2403.12945) · updated annotations on HF: [KarlP/droid](https://huggingface.co/KarlP/droid)
- **Scale [V]:** site abstract: "76k demonstration trajectories or 350h of interaction data, collected across 564 scenes and 86 tasks by 50 data collectors in North America, Asia, and Europe over the course of 12 months." 13 institutions use the same Franka Panda + Zed stereo + Oculus Quest 2 stack. Site update (Dec 2024): "3 natural language annotations for 95% of all successful DROID episodes (75k episodes)".
- **Language facts [V]:** paper: "Additionally each trajectory has the following metadata: • 1-3 natural language instructions describing the task performed in the trajectory, collected via crowdsourcing". Methods: "We provide up to three independently labeled instructions per episode from different crowd workers to ensure diversity of annotations." Quickstart code on the site reads `step["language_instruction"]` from the TFDS `droid` RLDS build.
- **Why it matters:** DROID is multi-annotation by design. Localization should preserve *which* annotation (worker, order) produced which string; collapsing three English instructions into one translated string destroys information a policy may use.
- **Confidence:** scale and annotation count [V]; English = **[I]** (all examples and the annotation platform are English-language; the paper never states the language).

### A.3 RT-1

- **Official pages:** [project site](https://robotics-transformer1.github.io/) · [paper arXiv:2212.06817](https://arxiv.org/abs/2212.06817)
- **Scale [V]:** site: "we collected a large dataset of real-world robotic experiences that consists of over 130k episodes, which contain over 700 tasks, and was collected with a fleet of 13 robots over 17 months."
- **Language facts [V/I]:** the model is language-conditioned: "RT-1 takes a short sequence of images and a natural language instruction as input". The paper defines instructions as "a verb surrounded by one or multiple nouns", e.g. "place water bottle upright", "move the coke can to the green chip bag". The OXE index classifies RT-1 as **"Templated"** language annotations.
- **Counting caution:** the OXE index lists "RT-1 Robot Action" as 73,499 RLDS episodes — a conversion count, *not* the 130k in the paper. Do not put both numbers next to each other without labels.
- **Confidence:** scale [V]; English = **[I]** (examples are English).

### A.4 BridgeData V2

- **Official pages:** [project site](https://rail-berkeley.github.io/bridgedata/) · [paper arXiv:2308.12952](https://arxiv.org/abs/2308.12952)
- **Scale [V]:** site: "60,096 trajectories / 50,365 teleoperated demonstrations / 9,731 rollouts from a scripted pick-and-place policy / 24 environments / 13 skills". WidowX 250, 5 Hz, average trajectory length 38 timesteps.
- **Language facts [V]:** paper methods: "Since we do not annotate trajectories with task names during data collection, we used a crowdsourcing platform to label the data post-hoc. Annotators were asked to describe the task being performed by the robot in each trajectory, with particular emphasis on the final location of any moved objects." OXE index classifies "Berkeley Bridge" as **"Natural"** annotations (listing 25,460 RLDS episodes — again a converted count).
- **Why it matters:** BridgeData V2 shows "natural" free-form annotations created *after* collection. Localization of post-hoc annotations is a different risk profile (no in-domain context for the translator), which is exactly the kind of thing a QA process should document.
- **Confidence:** scale/annotation method [V]; English = **[I]**.

### A.5 ALOHA / ACT

- **Official pages:** [ALOHA site](https://tonyzhaozh.github.io/aloha/) · [ACT paper arXiv:2304.13705](https://arxiv.org/abs/2304.13705) · [ACT code](https://github.com/tonyzhaozh/act)
- **Scale [V]:** ALOHA is a low-cost bimanual teleoperation system ($20k budget); ACT is trained from "50 demonstrations for each task"; the site reports four tasks at 96%, 84%, 64%, 92% success. OXE index: "ALOHA" 451 episodes, "MobileALOHA" 276 episodes, both classified "Templated"; Mobile ALOHA paper is [arXiv:2401.02117](https://arxiv.org/abs/2401.02117).
- **Language facts [V]:** ACT's policy input is "images from multiple viewpoints, joint positions, and style variable z" — there is **no language input**. Language-conditioning for these datasets exists only in downstream packaging (OXE index calls them "Templated"; LeRobot datasets add `task` strings).
- **Why it matters:** do not sell "translation of ALOHA instructions" as if ALOHA shipped instructions. Sell "adding a localized language layer when the dataset is packaged for language-conditioned training" — and say which format you are writing into.
- **Confidence:** all [V] from site/paper/index.

### A.6 LIBERO

- **Official pages:** [project site](https://libero-project.github.io/) · [paper arXiv:2306.03310](https://arxiv.org/abs/2306.03310) · [GitHub](https://github.com/Lifelong-Robot-Learning/LIBERO) · [HF datasets](https://huggingface.co/datasets/yifengzhu-hf/LIBERO-datasets)
- **Scale [V]:** README: "130 tasks grouped into four task suites: LIBERO-Spatial, LIBERO-Object, LIBERO-Goal, and LIBERO-100". Paper: "we provide 50 trajectories of high-quality demonstrations for every single task in the generated task suites" (130 × 50 = 6,500 demos).
- **Language facts [V/I]:** tasks carry a `language` attribute (`task.language` in the README example). Instructions are generated from templates — paper example: "Open the drawer of the cabinet" from the template "Open …"; the language is encoded with BERT in the reference policies. Format: HDF5; the loader reads `problem_info` from the HDF5 `data` group attrs and extracts `language_instruction` ([`dataset_utils.py` L43–45](https://raw.githubusercontent.com/Lifelong-Robot-Learning/LIBERO/master/libero/libero/utils/dataset_utils.py)).
- **Translation status:** no official multilingual release found. But LIBERO is the benchmark most often translated by researchers — a 2026 ACL paper translates it into **ten languages** [V, §B.2]. A "first to localize LIBERO" claim is false.
- **Confidence:** scale/format [V]; English = **[I]** (templates and examples are English).

### A.7 LeRobot (format + ecosystem)

- **Official pages:** [GitHub](https://github.com/huggingface/lerobot) · [dataset format v3 docs](https://huggingface.co/docs/lerobot/lerobot-dataset-v3) · [language columns docs (raw md)](https://raw.githubusercontent.com/huggingface/lerobot/main/docs/source/language_and_recipes.mdx) · [annotation pipeline docs](https://huggingface.co/docs/lerobot/annotation_pipeline) · [LeRobot paper arXiv:2602.22818](https://arxiv.org/abs/2602.22818) (Feb 2026)
- **Format facts [V]:** v3 layout includes `meta/info.json` (schema, fps, path templates), `meta/stats.json`, **`meta/tasks.jsonl` — "natural-language task descriptions mapped to integer IDs for task-conditioned policies"**, `meta/episodes/`, `data/chunk-*/file-*.parquet` and MP4 video shards. Optional language columns `language_persistent` and `language_events` live next to frame data in the parquet files, with row shape `role`, `content`, `style`, `timestamp`, `camera`, `tool_calls` [V, raw docs].
- **Language facts [V]:** docs: "Most LeRobot datasets ship with a single `task` string per episode." The `lerobot-annotate` tool "watches each episode's video with a vision-language model (VLM) and writes natural-language annotations back into your dataset. It fills the two language columns … straight into `data/chunk-*/file-*.parquet`." No translation, locale, or provenance fields are documented.
- **Why it matters:** LeRobot is the best candidate for a modern localized deliverable — it already separates task IDs from task text and already has language columns. A deliverable can add translated task text + per-episode language rows without forking the format. That is a concrete, checkable differentiator.
- **Confidence:** format/language mechanism [V]; "task strings are English in practice" = **[I]**.

### A.8 Adjacent corpora — honesty check (do not skip this)

Two large Asia-origin corpora also carry language annotations; both matter because they are the easiest counter-examples to an "English-only" claim.

- **AgiBot World Colosseo** — [paper arXiv:2503.06669](https://arxiv.org/abs/2503.06669): "over 1 million trajectories across 217 tasks in five deployment scenarios"; language annotations "for both the overall task and each individual sub-steps". Published examples are English ("make a coffee", "Pour Water"). The paper does **not** state an annotation language. Status: English examples [V], language [U]. Do not claim it is Chinese-language.
- **RoboMIND** — [paper arXiv:2412.13877](https://arxiv.org/abs/2412.13877): "107k demonstration trajectories across 479 diverse tasks involving 96 object classes" and "linguistic task descriptions"; four embodiments. Language not stated [U].
- **RxR** — [ACL 2020](https://aclanthology.org/2020.emnlp-main.356/): multilingual vision-and-language **navigation** dataset (English, Hindi, Telugu), per-word time alignment. It is the strongest published counter-example to "multilingual embodied instructions don't exist", and also shows the gap: navigation, not manipulation, and 2020.

**Implication for copy:** say "the major open manipulation corpora document English instructions"; do not say "all robot data is English". Mention navigation/multilingual data if you want the honest version.

---

## B. Prior art: multilingual robot instructions and dataset translation

**Bottom line:** translation/localization of robot instruction data is an *active research topic in 2026*, but what exists is (i) benchmark-side translations for evaluation, (ii) model-side language adaptation, (iii) isolated community dataset translations, and (iv) multilingual navigation data. We found **no** dataset-side localization product with published provenance/QA, and no official multilingual release of any of the seven datasets above. That is the credible white space — and it is smaller and more technical than "no one has done this".

### B.1 Prior-art table (10 links)

| # | Work | What it is | Why it matters here | Source |
|---|---|---|---|---|
| B1 | **RxR (Room-Across-Room)** — Ku et al., EMNLP 2020 | Multilingual VLN dataset; instructions in **English, Hindi and Telugu**; each word time-aligned to poses | Proves multilingual embodied instruction data is collectable and has existed since 2020 — but it is navigation, not manipulation | [ACL Anthology](https://aclanthology.org/2020.emnlp-main.356/) |
| B2 | **ReLI: Cross-Lingual Language-to-Action Grounding for HRI** (2025) | Cross-lingual HRI framework; 70K+ multi-turn conversations in 140+ languages; translates high-level robot control across languages | Shows the demand and the breadth; also states the problem precisely: existing HRI frameworks "typically support only a handful of high-resource languages, e.g., English and Chinese" | [arXiv:2505.01862](https://arxiv.org/abs/2505.01862) |
| B3 | **When Does Language Matter? Multilingual Instructions Reveal Step-wise Language Sensitivity in VLAs** (ACL 2026 Main; Dong et al.) | Translates **LIBERO into ten languages**; finds success drops **30–50%** under non-English instructions; proposes step-wise intervention | Closest direct prior art to "translate a robot benchmark". Also gives you a citable motivation for localization demand | [ACL Anthology](https://aclanthology.org/2026.acl-long.2066/) · [arXiv:2606.11906](https://arxiv.org/abs/2606.11906) |
| B4 | **Beyond English: Uncovering the Multilingual Gap in VLA Models** (2026) | Builds multilingual instructions by "extending existing benchmarks with translations"; systematic VLA evaluation; finds a "significant multilingual gap" even when the LLM backbone is multilingual | Confirms the problem is empirical and general, not a single-benchmark artifact | [arXiv:2606.15714](https://arxiv.org/abs/2606.15714) |
| B5 | **Measuring Language Transfer in Robot Policies: Adding Greek to a Cosmos3 VLA Policy** (2026) | Adds Greek via **machine-translated instructions**; reports measurement pitfalls; bilingual training +6.7–7.1 points over control but only ~2/5 of English; policy "overfits the translator's phrasing"; 7 phrasings/task "approximately halves this penalty" | The single most useful paper for a translation-QA pitch: translation is not enough; **phrasing diversity and measurement controls** decide the outcome | [arXiv:2609.07470](https://arxiv.org/abs/2609.07470) · [HF paper page](https://huggingface.co/papers/2609.07470) |
| B6 | **Arabic PushT Image Dataset** (community, HF) | "The first Arabic-translated dataset for Vision-Language-Action (VLA) robotics"; localized PushT with Arabic instructions; LeRobot-style `instruction` column; card claims 2.3× better accuracy on Arabic commands (claim not independently verified) | Proves one-off community dataset translations already exist — so "we are first to translate a robot dataset" is false | [HF dataset card](https://huggingface.co/datasets/hamzabouajila/ar-pusht-image) |
| B7 | **Sophea-Nano-Policy-LIBERO-Greek-v1** (community HF model) | Bilingual Greek/English LIBERO policy; card reports English 62.7%, unseen-Greek-phrasing 27.8%, **wrong-instruction control 21.4%** | Shows evaluation traps: a wrong instruction still scores 21.4%, so localized-policy gains must be measured against wrong-instruction controls, not just English baselines | [HF model card](https://huggingface.co/ayoubkirouane/Sophea-Nano-Policy-LIBERO-Greek-v1) |
| B8 | **LeRobot language columns + `lerobot-annotate`** (HF docs, 2026) | Two optional per-frame language columns (`language_persistent`, `language_events`) + recipe layer + VLM annotation tool that writes natural-language annotations into parquet | The infrastructure for language annotations already exists and is growing; translation/localization is *not* part of it. Concrete place to add value | [Language columns docs](https://huggingface.co/docs/lerobot/language_and_recipes) · [Annotation pipeline](https://huggingface.co/docs/lerobot/annotation_pipeline) |
| B9 | **Open X-Embodiment / RT-X** (2023) | Standardized multi-robot corpus and models; English task strings; index tracks Templated/Natural/None, no locale | The reference point every buyer knows. Its lack of a language field is your cleanest structural argument | [Project site](https://robotics-transformer-x.github.io/) · [Index sheet](https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/edit) |
| B10 | **ALLMAN: A German Vision-Language-Action Model** (TU Darmstadt; **unverified**) | A German-language VLA model effort surfaced in search, but the PDF was unreachable at research time (fetch timed out twice) | Treat as a **lead**, not a fact. If it matters, verify before citing. Do not put it on the site from this dossier | [PDF URL (unreachable 2026-10-22)](https://www.ias.informatik.tu-darmstadt.de/uploads/Team/MaximilianT%C3%B6lle/AllmAN_A_German_Vision_Language_Action_Model.pdf) — [U] |

### B.2 What we found that *does not* appear to exist (dated absence claims)

Searched 2026-10-22 on the web and Hugging Face; these are absences of evidence, not proofs.

- **No official multilingual release** of OXE/RT-X, DROID, RT-1, BridgeData V2, ALOHA/ACT, or LIBERO. LIBERO translations exist only inside research papers/benchmarks (B3) and community artifacts (B7). [I]
- **No language/locale field** in the OXE index or DROID metadata; LeRobot has language *columns* but no documented locale/provenance convention. [V for OXE/DROID/LeRobot docs]
- **No manipulation dataset** comparable in scale to OXE that is natively collected in a non-English language, beyond navigation (RxR, B1). We could not verify a single large manipulation corpus collected in, say, Hindi or Japanese. [I]
- **No dataset-translation product** with a published manifest, QA protocol, or per-episode provenance. Community one-offs (B6) and research translations (B3) exist; nothing that looks like a dataset-side service. [I]
- **No peer-reviewed paper** specifically titled/abstracted as "a pipeline for translating robot datasets" that we could find; the closest are benchmark-translation (B3), multilingual-gap (B4), and language-transfer (B5) papers. [I]
- **Not verified:** whether any OXE-contributing dataset (e.g. a Japanese or European lab corpus) contains non-English instruction strings. We checked dataset *cards*, not raw data dumps. [U]

### B.3 So what can Lean Robotics IQ credibly claim as different?

1. **Dataset-side, format-native localization with provenance.** Prior art is model-side (B5, B7) or benchmark-side (B3, B4). A deliverable that keeps episode IDs, task keys and annotation lineage — and publishes a manifest — is a different product. This is the strongest, most defensible differentiator.
2. **Validated paraphrase sets, not one literal translation.** Directly supported by B5: policies overfit the translator's phrasing, and seven phrasings per task approximately halve the penalty. A vendor selling "2–7 reviewed phrasings per task, each tagged and provenance-linked" is selling something the 2026 literature says actually matters.
3. **Measurement-aware QA.** B7's wrong-instruction control at 21.4% and B5's list of false instruments ("a color-histogram metric rewards noise… training loss fails to predict Greek success… single-run comparisons are dominated by seed variation") justify a QA protocol that includes wrong-instruction controls, multi-seed/multi-phrasing checks, back-translation, and native review. No competitor found publishes this.
4. **Honesty about scope** (a brand differentiator, not a technical one): state which formats and languages are supported now, publish failure modes, and refuse "100%" claims. Given the founder's own negative-result preprint, this is on-brand and hard to copy without the same evidence culture.

### B.4 Verbatim quotes that are safe to lift (with attribution)

Use with quotes + link; do not paraphrase these into stronger claims.

- **ACL 2026 (B3):** "by translating the LIBERO benchmark into ten languages, revealing severe performance degradation under non-English instructions, with success rates dropping by 30-50%."
- **Beyond English (B4):** "most existing VLA systems are trained and evaluated primarily with English instructions, leaving their ability to understand and execute instructions in other languages largely unexplored."
- **Greek transfer paper (B5):** "Robot foundation models are trained and evaluated predominantly in English, and robot demonstration corpora do not exist for most languages." … "Bilingual training yields a consistent 6.7-7.1 point margin over its control and reaches about two fifths of English performance." … "The policy also overfits the translator's phrasing; training on seven phrasings per task approximately halves this penalty."
- **ReLI (B2):** "Existing language-conditioned human-robot interaction frameworks typically support only a handful of high-resource languages, e.g., English and Chinese, limiting accessibility for billions of potential end users."
- **OXE project site (B9):** "It contains 1M+ real robot trajectories spanning 22 robot embodiments" and "pooling 60 existing robot datasets from 34 robotic research labs around the world."
- **DROID site (A.2):** "76k demonstration trajectories or 350h of interaction data, collected across 564 scenes and 86 tasks by 50 data collectors in North America, Asia, and Europe over the course of 12 months."
- **LIBERO paper (A.6):** "we provide 50 trajectories of high-quality demonstrations for every single task in the generated task suites."
- **LeRobot docs (A.7):** "Most LeRobot datasets ship with a single `task` string per episode."

---

## C. The market framing: "the instruction layer is overwhelmingly English"

### C.1 One defensible framing paragraph (≈110 words; every claim linked)

> The instruction layer of open robot data is documented in English. The Open X-Embodiment collaboration pooled 60 datasets from 34 labs and 22 robot embodiments — 527 skills across 160,266 tasks — and its dataset index records annotation style (templated, natural, none) but has no language column. DROID crowdsourced up to three English instructions per episode; RT-1, BridgeData V2 and LIBERO document English task strings. Yet when LIBERO was translated into ten languages for a 2026 ACL study, VLA success dropped by 30–50%, and a 2026 Greek case study found bilingual training recovered only about two-fifths of English performance. The gap is not that translation is impossible — it is that localization today is one-off, model-side, and rarely provenance-tracked.

Links to attach: OXE [site](https://robotics-transformer-x.github.io/) + [index](https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/edit) · [DROID](https://arxiv.org/abs/2403.12945) · [LIBERO](https://arxiv.org/abs/2306.03310) · [ACL 2026](https://aclanthology.org/2026.acl-long.2066/) · [Greek study](https://arxiv.org/abs/2609.07470).

### C.2 Strongest supporting citations (ranked)

1. **OXE index has no language column** — a structural fact about the field's central corpus, not a survey claim. [V]
2. **ACL 2026 translation study: 30–50% success drops** — the best single number to motivate localization. [V]
3. **Greek 2026 study: translation alone isn't enough; phrasing variety matters** — motivates QA/paraphrase services. [V]
4. **ReLI: existing frameworks support only a handful of high-resource languages** — demand-side evidence. [V]
5. **DROID's 1–3 annotations per episode** — proves instruction metadata is already multi-source, so provenance matters. [V]

### C.3 The strongest overclaim to avoid

> "Robot datasets are 100% English-only" / "no multilingual robot data exists" / "we are the first to translate robot data."

Why it fails: RxR is multilingual since 2020 ([B1](https://aclanthology.org/2020.emnlp-main.356/)); LIBERO has been translated into ten languages for evaluation ([B3](https://aclanthology.org/2026.acl-long.2066/)); an Arabic PushT dataset is public ([B6](https://huggingface.co/datasets/hamzabouajila/ar-pusht-image)); the Greek study exists ([B5](https://arxiv.org/abs/2609.07470)). Also, no one has published a dataset-by-dataset language census, so no percentage is defensible.

### C.4 Three safe phrasings (use these)

1. **"The open robot datasets that anchor today's VLA models document English-language instructions; we found no official multilingual splits of any of them (checked 2026-10-22)."**
2. **"Multilingual robot-instruction research is active in 2026, but most of it translates benchmarks for evaluation or adapts models. Dataset-side localization with provenance is the part that is missing."**
3. **"English is the default instruction language of the open corpora. Localization today is mostly one-off and model-side; we do the dataset-side work and publish what we changed."**

Plus a fourth if the site needs punch: **"We audited the documentation, not the raw data. Where the language isn't stated, we say so."** (This is the honest version of C.4.1 and matches this dossier's method.)

---

## D. Formats: what a localized deliverable actually looks like

This is the section to give an engineer. For each format: what the instruction is, where it lives, what "localized" should preserve, and the failure modes.

### D.1 RLDS (the substrate under OXE, DROID, BridgeData V2)

- **What it is [V]:** RLDS = Reinforcement Learning Datasets; a dataset is a `tf.data.Dataset` of episodes, each episode a dataset of steps. Steps have mandatory `is_first` / `is_last`; optional `observation`, `action`, `reward`, `is_terminal`, `discount`, plus extra metadata. Episodes carry user-defined metadata; the spec proposes `episode_id`, `agent_id`, `environment_config`, `experiment_id`, `invalid`. Source: [RLDS README](https://github.com/google-research/rlds).
- **Where instructions live:** the RLDS robot datasets in this ecosystem carry a per-step `language_instruction` field — DROID's official quickstart reads exactly `step["language_instruction"]` ([DROID site](https://droid-dataset.github.io/)), and OXE models receive instructions as part of the observation stream. The string is typically repeated across steps in an episode.
- **Localized deliverable:** a new RLDS/TFDS build (or a patched copy) where:
  - every `language_instruction` is replaced by the target-language string;
  - `is_first` / `is_last` / actions / observations / episode boundaries are untouched (byte-identical arrays);
  - the original string is preserved in a sidecar for traceability;
  - a manifest maps `(dataset, episode_id, step_span) → {src, tgt, translator, reviewer, method, confidence}`.
- **Failure modes:** changing step counts or episode boundaries; losing the multi-annotation structure (DROID has up to 3 annotations — flattening loses information); translating a templated annotation as if it were natural language; no record of which annotation was translated.

### D.2 OXE / RT-X schema specifics

- **What it is [V]:** standardized observation/action spaces across 22 embodiments; the model receives "a history of recent images and language instructions as observations" and predicts a 7-D end-effector action; models are RT-1-X and RT-2-X. Instructions are part of the observation stream, not a separate language corpus.
- **Localized deliverable:** translate the instruction stream only; do not touch action/observation normalization. Document which OXE datasets were translated, and their annotation style (`Templated` / `Natural` / `None` from the [index](https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/edit)). Datasets marked `None` need an annotation step before translation — a different, lager job; price it separately and say so.

### D.3 DROID (RLDS + raw recordings)

- **What it is [V]:** TFDS `droid` in RLDS form on `gs://gresearch/robotics` ([quickstart](https://droid-dataset.github.io/)); raw release documented as JPEG/PNG/pkl + camera calibration; each trajectory has "1-3 natural language instructions … collected via crowdsourcing" and up to 3 independent labels per episode on the [tasq.ai](https://tasq.ai) platform ([paper](https://arxiv.org/abs/2403.12945)).
- **Localized deliverable:** translated instruction variants with the source annotation index preserved (annotation 1/2/3, worker id if exposed, crowd platform, date); updated annotation set published like the Dec 2024 HF release ([KarlP/droid](https://huggingface.co/KarlP/droid)) with a versioned card and license note.
- **Failure modes:** silently replacing all 3 annotations with 1 translated string; losing the provenance of which annotation came from which worker; no license/attribution review for the original annotations.

### D.4 LeRobot dataset format v3

- **What it is [V]:** `meta/info.json` (schema, fps, path templates), `meta/stats.json`, `meta/tasks.jsonl` ("natural-language task descriptions mapped to integer IDs"), `meta/episodes/`, `data/chunk-*/file-*.parquet`, per-camera MP4 shards. Optional language columns `language_persistent` / `language_events` in the parquet, rows shaped `role`, `content`, `style`, `timestamp`, `camera`, `tool_calls` ([format docs](https://huggingface.co/docs/lerobot/lerobot-dataset-v3), [language docs](https://raw.githubusercontent.com/huggingface/lerobot/main/docs/source/language_and_recipes.mdx)).
- **Localized deliverable:** translated `meta/tasks.jsonl` (keeping the integer IDs and staying consistent with the `task`/`task_index` column in parquet), plus optional localized `language_persistent` / `language_events` rows, plus a new `meta/` provenance file (source dataset hash, translator, reviewer, glossaries, coverage). If you add a new language as a separate dataset copy, keep `task_index` stable so training configs don't break.
- **Failure modes:** translating the task string but leaving `task_index`/parquet references mismatched; overwriting the original task text so the source is unrecoverable; adding language columns without documenting their semantics; bumping the format version without saying so.

### D.5 LIBERO HDF5

- **What it is [V]:** HDF5 demo files per task/suite; the loader opens the file, reads `problem_info` from `f["data"].attrs`, JSON-decodes it, and joins `problem_info["language_instruction"]` ([`dataset_utils.py` L43–45](https://raw.githubusercontent.com/Lifelong-Robot-Learning/LIBERO/master/libero/libero/utils/dataset_utils.py)). Task suites expose `task.language` ([README](https://github.com/Lifelong-Robot-Learning/LIBERO)).
- **Localized deliverable:** per-task translated `language_instruction` written into a copy of the HDF5 `problem_info` attrs (or an overlay mapping), plus a task-suite-level mapping table (130 entries × suite) with source string, target string, reviewer, and demo count. Because one string is repeated across 50 demos, translate at task level and verify the join.
- **Failure modes:** editing each demo independently and drifting between demos of the same task; losing the task ID ↔ language mapping; changing the HDF5 structure so the official loader breaks; no versioning of the localized artifact.

### D.6 ALOHA / ACT HDF5

- **What it is [V]:** ACT repo records `.hdf5` episodes (action + images); the policy is not language-conditioned (no language input), and the ALOHA project page documents image/joint inputs only ([ALOHA](https://tonyzhaozh.github.io/aloha/), [ACT repo](https://github.com/tonyzhaozh/act)).
- **Localized deliverable:** there is nothing native to translate. A localization product for ALOHA is an *annotation layer*: task names/instructions added during conversion (e.g. into LeRobot format), then localized. Sell that explicitly as "annotation + localization", not "translation of ALOHA instructions".
- **Failure modes:** implying ALOHA shipped instructions; adding language columns without an equivalent English baseline; claiming compatibility with ACT training when ACT cannot consume language.

### D.7 JSONL / JSON manifests (the glue format)

- **What it is:** the standard way to ship instruction mappings and annotations (LeRobot's `meta/tasks.jsonl` is the canonical example, [docs](https://huggingface.co/docs/lerobot/lerobot-dataset-v3)); JSONL also appears in rubric/annotation tooling.
- **Localized deliverable (recommended canonical manifest rows):**

```json
{"dataset":"lerobot/aloha_static_cups_open","dataset_version":"v3.0","episode_id":12,"task_index":0,
 "src_lang":"en","tgt_lang":"el","src_text":"open the cup","tgt_text":"άνοιξε το κύπελλο",
 "method":"human-translation","translator":"T-014","reviewer":"R-003","back_translation":"open the cup",
 "status":"approved","source_hash":"sha256:…","notes":"phrasing set A"}
```

- **Why it matters:** this row shape is exactly what a provenance claim needs. If the site says "auditable", this is the artifact that makes it true — publish a redacted sample.
- **Failure modes:** manifest without source hash; missing reviewer identity; "approved" with no back-translation; translations without dataset version pins.

### D.8 Non-negotiable checklist for any localized artifact

1. Source dataset + exact version + hash recorded.
2. Original strings preserved and addressable (never overwrite in place without a retained source copy).
3. Episode/step/task IDs preserved; nothing about actions/observations changed.
4. Per-string provenance: translator, reviewer, method, date, status.
5. Back-translation or independent review recorded for each string.
6. Coverage report: translated / total, by dataset and by annotation style.
7. License and attribution for the source data checked before redistribution.
8. Failure log published (what was hard, what is uncertain).

---

## E. Exact paper facts, with section references and plain-English glosses

**Read this first:** the preprint is about an **autonomous coding agent**, not robots and not translation. It contains no robot data, no multilingual experiment, and no localization evaluation. Any use on a robotics site is an *adaptation of principles*, not a report of robotics results. [V — title, abstract, §7.1]

### E.0 Citation identity and status (get this exactly right)

| Item | Value | Source | Conf. |
|---|---|---|---|
| Author | Dušan Milosavljević | paper cover; ORCID | [V] |
| Affiliation | Praktess Research, Aura Code Project | paper cover | [V] |
| ORCID | [0009-0006-4706-0467](https://orcid.org/0009-0006-4706-0467) | ORCID record | [V] |
| Correspondence | aura@leanproiq.com | paper cover | [V] |
| ORCID work type | `preprint` | [ORCID public record](https://pub.orcid.org/v3.0/0009-0006-4706-0467/record) | [V] |
| ORCID work date | 2025-10-04 | same | [V] |
| ORCID external IDs | **none** (no DOI) | same | [V] |
| ORCID work URL | `https://elub-portfolio.vercel.app/abstract` | same | [V] |
| ORCID other activities | 1 work total; 0 employments; 0 peer reviews; **1 education entry** (University of Novi Sad, BSc Electrical & Information Engineering, 1996–2000); **1 qualification entry** (Google Skills MLOps badge, Apr–Jun 2026) — both self-asserted by the author | same | [V] |
| Author's abstract page | states **"Preprint · Not peer reviewed"**, dated 10 September 2025, PDF at `/paper/recursive-learning-archimedes-principle.pdf` | [abstract page](https://elub-portfolio.vercel.app/abstract) | [V] |
| Paper's own status line | "Manuscript status: Preprint. Not peer reviewed. Contains a pre-registered protocol (Section 8) that has not yet been executed." | paper front matter; `paper.tex` line 118 | [V] |
| Current PDF version | dated **22 October 2026** | `/mnt/bigdata/aura/projects/orcid-paper/paper.pdf` + `paper.tex` | [V] |
| Version drift | 2025-dated and 2026-dated PDFs differ **only in the cover date** (pdftotext diff = 4 lines); all numbers identical | local diff 2026-10-22 | [V] |
| DOI / arXiv / journal | none found (web search 2026-10-22; ORCID has no external IDs) | — | [U]/none |
| Google Scholar | profile returned HTTP 429 to automated fetch; no citation counts verified | [profile](https://scholar.google.com/citations?user=Gbp13nYAAAAJ&hl=en) | [U] |

**Recommended citation for the site footer/about page:**

> Milosavljević, D. *Recursive Learning and the Archimedes Principle: Competence-Gated Tier Alternation as a Cost–Quality Mechanism for Autonomous Coding Agents.* Preprint (2025; current version 22 October 2026), not peer reviewed. Praktess Research / Aura Code Project. ORCID [0009-0006-4706-0467](https://orcid.org/0009-0006-4706-0467). [Abstract](https://elub-portfolio.vercel.app/abstract)

**Do not write:** "published paper", "peer-reviewed study", "journal article", "DOI: …", "cited N times", "peer-reviewed research from Praktess".

### E.0.1 Founder credentials (only if the site publishes a bio)

The public ORCID record (fetched 2026-10-22) contains two credential entries, both **self-asserted by the author** (ORCID source = the record owner):

- **Education:** University of Novi Sad, BSc in Electrical and Information Engineering, Sep 1996 – Dec 2000. [ORCID record](https://orcid.org/0009-0006-4706-0467)
- **Qualification:** "MLOps (Machine Learning Operations) Engineer", department "Generative AI", Google (Israel), Apr–Jun 2026, with a public Google Skills badge URL. The badge resolves to a specific course: *Machine Learning Operations (MLOps) with Agent Platform: Model Evaluation*.[badge](https://www.skills.google/public_profiles/3100cf0f-c7d7-4322-b373-2ff62c2a30a4/badges/27107673)]

**Safe copy:** "Dusan Milosavljević holds a BSc in Electrical and Information Engineering from the University of Novi Sad (per his ORCID record) and has completed a Google Cloud MLOps skills badge."

**Unsafe copy:** "ex-Google engineer", "Google ML engineer", "worked at Google", "Google researcher". ORCID's **employment** section is empty; the Google entry is a course qualification/badge, not a job. Do not upgrade a badge into employment. Education should be attributed ("per ORCID") because ORCID entries are self-asserted.

### E.1 Audit numbers (all from §7 unless noted; quote with the label "founder's audited coding-agent deployment")

| Fact | Exact value | Location | Plain-English gloss for copy | Site-safe? |
|---|---|---|---|---|
| Episode corpus size | **1,225** episode records | §7.1, Table 3 | The deployment stored 1,225 recorded runs. | Only with coding-agent label |
| Project stores | **20** project directories | §7.1, Table 3 | Spread over 20 codebases/projects. | Only with label |
| Observation window | **2026-09-13 → 2026-10-17** | §7.1, Table 3 | About five weeks of ordinary use. | Prefer to omit dates |
| Model-labelled records | **1,072** | §7.1, Table 3 | Runs where the model that produced the answer was recorded. | Only with label |
| Full alternation schema | **153** records | §7.1, Table 3 | The subset instrumented for the tier-routing experiment. | Only with label |
| Successful outcomes | **835 (68.2%)** | §7.1, Table 3 | The run's own success flag — the paper explicitly says this must not be read as a capability estimate. | No, unless quoted with caveat |
| Total tokens attributed | **1,049,656,173** | §7.1, Table 3 | Total token usage in the corpus. | Only with label |
| Median tokens per episode | **80,381** | §7.1, Table 3 | Typical run cost, in tokens. | Only with label |
| Small tier attempted | **0 / 153 (0.0%)** | §7.2, Table 4 | The cheap tier never ran. | With label, yes |
| Small tier succeeded | **0 / 153 (0.0%)** | §7.2, Table 4 | — | With label |
| Escalated to large tier | **153 / 153 (100.0%)** | §7.2, Table 4 | Every instrumented task went to the large model. | With label |
| Escalated answer approved | **143 / 153 (93.5%)** | §7.2, Table 4 | Most escalated answers were approved; approval ≠ independently verified correct (§9). | With label + caveat |
| Ledger rows `direct-large` | **143 / 143 (100.0%)** | §7.2, Table 4 | The cost ledger is unanimous: no cheap-tier rows. | With label |
| Ledger rows with non-null small-model field | **0 (0.0%)** | §7.2, Table 4 | No cheap-tier token counts exist. | With label |
| Tasks replayed for gate reconstruction | **143** | §7.3, Table 5 | The gate's own similarity rule was replayed over the ledger. | With label |
| No prior similar task | **121 (84.6%)** | §7.3, Table 5 | Most tasks were unlike anything before them. | With label |
| 1–2 prior similar tasks | **22 (15.4%)** | §7.3, Table 5 | The rest had at most two neighbours. | With label |
| ≥3 prior similar tasks | **0 (0.0%)** | §7.3, Table 5 | Nothing reached the evidence threshold. | With label |
| Neighbourhood median / mean / max | **0 / 0.18 / 2** | §7.3, Table 5 | Typical task had zero similar predecessors. | With label |
| Tasks reaching decision stage | **0 of 143** | §7.3, Table 5 | The gate never evaluated a success rate. | With label |
| Distinct pattern keys | **143 of 143** | §7.3, Table 5 | Every task produced a unique key — K ≈ n, worst case for the gate. | With label |
| Distinct 8-token slugs | **143 of 143** | §7.3, Table 5 | Same, at the slug level. | With label |
| Prompt tokens (ledger) | **6,943,053** | §7.4, Table 6 | Context re-sent to the model. | With label |
| Completion tokens (ledger) | **131,618** | §7.4, Table 6 | Generated tokens. | With label |
| Total tokens (ledger) | **7,074,671** | §7.4, Table 6 | The cost of the always-escalate regime. | With label |
| Mean tokens per task | **49,473** | §7.4, Table 6 | Average cost per routed task. | With label |
| Wall time median | **8.6 s** | §7.4, Table 6 | Typical latency. | With label |
| Wall time mean / max | **14.3 s / 91.3 s** | §7.4, Table 6 | — | With label |
| Prompt:completion ratio | **53 : 1** | §7.4 | Diagnostic of agentic work: context dominates. | With label |
| Model labels | 7 shown; rates 0.75–0.92 | §7.5, Table 7 | Mixed providers; paper says confounded by task mix and not a ranking. | No (ranking bait) |

**The paper's six findings (§7.6), quoted in substance:** (1) gate never exercised (0/153 attempts); (2) structurally unreachable (max 2 neighbours vs 3 required); (3) pattern vocabulary doesn't repeat (143 unique keys/slugs, K≈n); (4) fallback tier adequate but unexamined (93.5% approved, no comparison); (5) price of the null regime is full price (7.07M tokens at 49.5k/task, median 8.6 s); (6) non-identifiability appeared exactly as predicted.

**One-line gloss for the whole audit:** *"The founder ran a real month-long coding-agent deployment, instrumented it honestly, and reported that the cost-gating mechanism never fired — then showed the theory predicts that."* This is a credibility asset **only** if the site is explicit that it is a coding-agent audit.

### E.2 Propositions (theory) — exact statements and glosses

| Ref | Statement (abridged, exact symbols) | §  | Plain-English gloss | Site-safe? |
|---|---|---|---|---|
| **Prop. 4.3** | Break-even: `E[Cgate] < E[CL]` iff `q > cS / cL`, where `q = pρ + (1−p)α`; relative saving `1 − (q − cS/cL)` | §4.3 | A cheap tier pays for itself only when its *verified* acceptance rate beats the tier price ratio. | Yes, as "the preprint's break-even inequality `q > cS/cL`" |
| **Prop. 6.6** | Consistency / sample complexity: `P[|ŝk − pk| ≥ η] ≤ 2exp(−2nkη²)`; need `nk ≥ (1/2η²)ln(2/δ)` | §6.2 | You need a lot of examples of the *same kind* of task before a success-rate estimate can be trusted. | Yes, with gloss |
| **Cor. 6.7** | Gate error ≤ `2exp(−2nk(pk−τ)²)` | §6.2 | Gating is reliable in proportion to how far a pattern is from the threshold, not to total experience. | Yes |
| **Prop. 6.9** | Probe: expected `εN` probes, expected overhead `εΔcN`; `P[no probe in N] = (1−ε)^N`; no permanent exclusion | §6.2 | A small random probe rate keeps excluded patterns from being excluded forever, at bounded cost. | Yes |
| **Prop. 6.11** | Safety envelope: `P[system error] ≤ (1−pk)α + (1−pL)`; with perfect verifier (α=0) gated ≤ always-escalate; target εt feasible only if `εt > 1−pL` | §6.2 | The gate cannot be safer than its verifier; the verifier's false-accept rate is the whole quality story. | Yes — this is the best "QA matters" citation |
| **Prop. 6.13** | Contraction: if distillation is λ-Lipschitz (λ<1) and injection non-expansive, `R` is a contraction with unique fixed point; noisy iterates enter ball `σ/(1−λ)` | §6.2 (conditional on Assumption 6.4) | Memory is stable only if summarising shrinks differences — *assumed, not proven*, for LLMs. | Yes, with "conditional" |
| **Prop. 6.14** | Reachability: expected arrivals until evaluable = `m0/πk`; equiprobable patterns → `Θ(m0K)`; evaluable fraction = binomial tail | §6.2 | When every task looks unique, the gate never accumulates enough evidence to open. | Yes |
| **Prop. 6.16** | Non-identifiability: if unavailable attempts are recorded as `attempted=0`, then `nk=0` for all k, `ŝk` undefined, gate stays in learning branch for all n, decision independent of true `pk` | §6.2 | If "we couldn't try" is recorded the same as "we chose not to try", the system can never learn why — and looks fine from outside. | Yes |
| **Assumption 6.4** | Distillation contraction (λ<1) — "the least secure in the paper"; "no evidence is offered that LLM consolidation is a contraction" | §6.1, §9 | The stability result rests on an unmeasured premise. | Must include if citing 6.13 |
| **Table 2** | Epistemic status: 6.6/4.3/6.9/6.11/6.14/6.16 "proved"; 6.13 "proved conditional" | §6.3, Table 2 | Which results are theorems and which have an empirical premise. | Yes |

### E.3 Pre-registered protocol (not executed) — the honest "what we'd do" list

- **Arms (§8.1):** A = always escalate (baseline); B = frozen gate, ε=0; C = full system, ε=0.05; D = floor (always attempt small, escalate only on verifier rejection). Two populations: *recurring* (tasks repeated with variation) and *open-ended* (natural low-recurrence stream).
- **Metrics (§8.2):** primary = cost per completed task (tokens × price + amortised verifier cost) and task success under blind adjudication; secondary = accepted-error rate `α̂` from a stratified sample of 200 accepted small-tier answers, two raters blind to arm (report κ), escalation rate, latency, gate quality vs an oracle estimated from arm D.
- **Hypotheses (§8.3):** H8.1 cost — on recurring population, `E[CC] ≤ 0.8·E[CA]`, paired Wilcoxon one-sided α=0.05. H8.2 quality non-inferiority — success in C not worse than A by more than δ=0.02. H8.3 verifier soundness — false-accept ≤0.02. H8.4 gate calibration — ≥90% agreement with oracle + Brier score. H8.5 probe recovers competence faster after induced backend regression. H8.6 recursion contractive — `λ̂<1`. **Negative control:** on open-ended population C vs A differ by <5%, else the harness is suspect.
- **Power (§8.4):** per-pattern η=0.10 at 95% needs `nk ≈ 184`; with K=100 patterns, `N ≈ 18,400` tasks for *one* pattern's tight evidence; paired design N≈282 (σd=0.12, δ=0.02); cost hypothesis N≈64 (σd≈0.4).
- **Ablations (§8.5):** (a) threshold sweep τ∈[0.5,0.9]; (b) probe sweep ε∈{0,0.01,0.05,0.2}; (c) verifier replaced by the author model (should raise α̂ and error); (d) pattern-key granularity flat vs hierarchical.
- **Status [V]:** "Contains a pre-registered protocol (Section 8) that has not yet been executed." → On the site: *"a pre-registered protocol is specified, not yet run."*

### E.4 Configuration constants (Appendix C) — useful if the site describes the method

`modelName=qwen2.5-coder:1.5b` · `competenceThreshold τ=0.70` · `minAttempts m0=3` · `epsilonProbeRate ε=0.05` · `similarity threshold τs=0.35` · `MAX_EXAMPLES=10` · `DEFAULT_MIN_FAILURES=20` · `small-tier turn cap=15` (never raised) · recent episode window=50 · local endpoint timeout=3000 ms. Source: Appendix C.

### E.5 What the paper explicitly does **not** claim (use to police copy)

- §1.4: "It does not claim that a small local model can replace a frontier model on real repository work; no controlled experiment in this paper tests that." · "It does not claim that recursive learning improves task success; the convergence result is conditional on Assumption 6.4, which we did not measure." · "It does not claim novelty for cascades, deferral, or distillation individually."
- §4.4 anti-claims: A1 not a claim about model physics (no hydrodynamic quantity); A2 does not assert small models suffice; A3 not a compression claim about weights; A4 confers no quality guarantee by itself.
- §7.7: "It cannot establish that gating saves money, that a 1.5B local model reaches the threshold on recurring patterns, that the verifier is sound, or that consolidation is contractive. It contains no arm in which the small tier ran, so the central mechanism of the paper is, empirically, untested here."
- §11: "Until that study exists, the Archimedes Principle should be read as a well-posed hypothesis with a working implementation – not as a result."

### E.6 The paper ↔ robotics gap, and the safe way to bridge it

**Gap [V]:** the paper's tasks are "natural-language engineering requests" (§3.1) handled by an autonomous coding agent; the audit is of the "Aura Code deployment" (§7.1). There is no robot, no dataset, no translation, and no localization content anywhere in the paper.

**Safe bridge (recommended wording):**

> Lean Robotics IQ is built on two ideas from the founder's preprint — *recursive learning* (keep a provenance-linked memory of decisions and consolidate it into future work) and the *Archimedes Principle* (let a cheap tier attempt the work first, accept only after independent verification, and escalate on failure). The preprint studies those ideas in an autonomous coding agent. We apply them to robot-dataset localization. We do not claim the preprint validated this application; that is our work to demonstrate.

**Unsafe bridge (do not write):** "Our method is scientifically proven on robotics data", "peer-reviewed research shows our translation QA works", "the Archimedes Principle reduces localization cost by 20%".

---

## F. "Do not claim" list + safe rewrites

### F.1 Hard bans (no exceptions)

1. **Peer review / venue.** No "peer-reviewed", "published in", "journal article", "conference paper", "accepted at", "DOI", "cited N times". Evidence: E.0.
2. **Proof language.** No "proven", "scientifically proven", "validated", "research shows our method works", "evidence-based" (unless pointing to a specific cited paper about *that* claim).
3. **Performance/savings numbers.** No "20% cheaper", "30% faster", "X% quality improvement" attributed to us. The 20% is an unrun pre-registered hypothesis (E.3); the 30–50% is a *degradation* reported by an external ACL 2026 study (B3) and must be attributed.
4. **First / only / nobody.** No "first", "only", "nobody else", "world's leading", "the standard". Counter-evidence: B1, B3, B5, B6.
5. **Universals about language.** No "100% English", "all robot datasets", "every dataset". Say "the open corpora we reviewed document English instructions; we found no official multilingual release".
6. **Customers / logos / testimonials / funding.** None exist in the sources reviewed. No "trusted by", "our clients", "backed by", "partners include".
7. **Coverage figures.** No "supports N languages", "N datasets localized", "N million strings" unless a real, auditable number exists. No ISO/SOC 2/security or "enterprise-grade" claims without evidence.
8. **Dataset ownership/partnership.** No implying we own, co-authored, or are endorsed by OXE/RT-X, DROID, RT-1, BridgeData V2, ALOHA, LIBERO, LeRobot, AgiBot World, RoboMIND.
9. **Reusing the audit as a robotics/translation result.** The 1,225 episodes / 143 tasks / 0 attempts are a coding-agent deployment audit; they are not evidence about translation quality or robot learning.
10. **"Published method"** without "preprint". The only correct framing: "the founder's preprint (not peer reviewed)".
11. **Employment/credential inflation.** No "ex-Google", "Google engineer", "worked at Google", "Google researcher". ORCID lists a Google Skills **course badge** under *qualifications* and **no employment** (§E.0.1); that is not a job. Education may be stated only as "per his ORCID record" (self-asserted).

### F.2 Risky copy → safe rewrite (use the right column)

| Risky (do not ship) | Safe version | Basis / link |
|---|---|---|
| "Peer-reviewed research from our founder" | "The founder's preprint on cost-gated inference (not peer reviewed)" | [Abstract page](https://elub-portfolio.vercel.app/abstract) |
| "Our method is proven to cut translation costs" | "The preprint derives a break-even condition for cost-gated work and pre-registers a cost study; that study has not been run." | §4.3, §8.3 |
| "100% of robot data is English-only" | "The open robot corpora we reviewed document English-language instructions; we found no official multilingual releases (checked 2026-10-22)." | §A, §C.3 |
| "No one has translated robot datasets before" | "Multilingual robot-instruction research is active, but most of it translates benchmarks for evaluation or adapts models. Dataset-side localization with provenance is what we do." | §B.2, §B.3 |
| "We're the first to localize LIBERO" | "LIBERO has been translated into ten languages in research evaluation; we localize datasets for release, with provenance." | [ACL 2026](https://aclanthology.org/2026.acl-long.2066/) |
| "Our translations boost robot success by X%" | "A 2026 study reports 30–50% success drops when LIBERO instructions are translated; closing that gap is the problem we work on." | [ACL 2026](https://aclanthology.org/2026.acl-long.2066/) |
| "We support 40+ languages" | "Language coverage is agreed per project and reviewed by native speakers; we publish the coverage of each delivery." | — (policy, not a fact) |
| "AI-powered, fully automated translation" | "Machine translation is one step; independent verification and native review are part of every delivery." | Prop. 6.11 idea; B5 |
| "Trusted by leading robotics labs" | Omit entirely until there are real, attributable customers. | — |
| "Our audit shows the gate saves money" | "The founder's audited coding-agent deployment found the gate never fired; the paper reports this as a negative result." | §7.6, §11 |
| "Trained on 1M+ robot episodes" | Never say this. Lean Robotics IQ has no such dataset. | — |
| "Published method, available in our product" | "Design principles from the founder's preprint inform our QA workflow." | E.6 |

### F.3 Claims that *are* safe today (ready to use, with citations)

1. "The founder's preprint is public and lists its own limitations, including a negative audit result." — [abstract](https://elub-portfolio.vercel.app/abstract) + §7.7.
2. "The preprint's central cost result is a single inequality: a cheap tier is worth using when its *verified* acceptance rate exceeds the price ratio `cS/cL`." — §4.3.
3. "The preprint's safety result says a gated system can never be safer than its verifier." — Prop. 6.11, §6.2.
4. "Open X-Embodiment standardizes data from 60 datasets and 22 robot embodiments, and its dataset index has no language/locale column." — [OXE site](https://robotics-transformer-x.github.io/) + [index](https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/edit).
5. "DROID carries 1–3 crowdsourced instructions per episode, up to three independent labels." — [DROID paper](https://arxiv.org/abs/2403.12945).
6. "A 2026 ACL study translated LIBERO into ten languages and found 30–50% success drops under non-English instructions." — [ACL 2026](https://aclanthology.org/2026.acl-long.2066/).
7. "A 2026 language-transfer study found robot policies overfit the translator's phrasing; more phrasings per task reduced the penalty." — [arXiv:2609.07470](https://arxiv.org/abs/2609.07470).
8. "LeRobot's format already has task IDs, optional language columns, and an annotation tool — but no translation or locale provenance layer." — [format docs](https://huggingface.co/docs/lerobot/lerobot-dataset-v3) + [language docs](https://huggingface.co/docs/lerobot/language_and_recipes).
9. "A localized deliverable should preserve episode/task IDs, keep the source strings, and record translator/reviewer/back-translation per string." — this dossier, §D.8 (service promise, not a factual claim about the world).
10. "We publish what we changed and what we could not verify." — service promise; on-brand with the founder's negative-result preprint.

---

## G. Source table

All URLs fetched 2026-10-22 unless noted. "Supports" = the specific facts this source backs in this dossier.

| ID | Source | URL | Type | Supports | Conf. |
|---|---|---|---|---|---|
| S1 | ORCID public record (JSON API) | https://pub.orcid.org/v3.0/0009-0006-4706-0467/record | official registry | Author identity, 1 work, preprint type, 2025-10-04, no external IDs, no peer reviews/employments | [V] |
| S2 | Author's abstract page | https://elub-portfolio.vercel.app/abstract | author site | "Preprint · Not peer reviewed"; 10 Sept 2025; PDF link | [V] |
| S3 | Paper PDF + LaTeX (current version) | `/mnt/bigdata/aura/projects/orcid-paper/paper.pdf`, `paper.tex` | primary | All §E numbers, props, protocol, constants, non-claims | [V] |
| S4 | Paper (2025-dated copy, .work) | `/mnt/bigdata/aura/projects/leanproiq-site/.work/recursive-learning.pdf` (+ `.txt`) | primary | Version comparison (cover date only differs); same numbers | [V] |
| S5 | OXE project site | https://robotics-transformer-x.github.io/ | official | 1M+ trajectories, 22 embodiments, 60 datasets, 34 labs, 527 skills / 160,266 tasks, English task-string examples | [V] |
| S6 | OXE paper (arXiv) | https://arxiv.org/abs/2310.08864 | paper | 22 robots, 21 institutions, 527 skills (160266 tasks), language-instruction observations, language annotations used for analysis | [V] |
| S7 | OXE dataset index (spreadsheet) | https://docs.google.com/spreadsheets/d/1rPBD77tk60AEIGZrGSODwyyzs5FgCU9Uz3h-3_t2A9g/edit | official dataset index | 72 datasets, 2,419,193 episodes, 8,964.94 GB; "Language Annotations" = Templated 26 / Natural 15 / None 26 / other 5; no locale column | [V] |
| S8 | OXE GitHub | https://github.com/google-deepmind/open_x_embodiment | official code | Dataset list/spreadsheet pointer; RT-1-X input = RGB + task string | [V] |
| S9 | DROID project site | https://droid-dataset.github.io/ | official | 76k trajectories / 350 h, 564 scenes, 86 tasks, 50 collectors, N. America/Asia/Europe, 12 months, 13 institutions; 3 annotations for 95% of successful episodes; `step["language_instruction"]` quickstart | [V] |
| S10 | DROID paper (arXiv) | https://arxiv.org/abs/2403.12945 | paper | 1–3 crowdsourced instructions/trajectory; tasq.ai; up to 3 independent labels; three camera views + language annotations | [V] |
| S11 | DROID updated annotations (HF) | https://huggingface.co/KarlP/droid | official release | Dec 2024 annotation update; camera calibrations for 36k episodes (site) | [V] |
| S12 | RT-1 project site | https://robotics-transformer1.github.io/ | official | 130k+ episodes, 700+ tasks, 13 robots, 17 months; language instruction input; example instructions | [V] |
| S13 | RT-1 paper (arXiv) | https://arxiv.org/abs/2212.06817 | paper | Instruction definition (verb + nouns), 700+ language instructions | [V] |
| S14 | BridgeData V2 site | https://rail-berkeley.github.io/bridgedata/ | official | 60,096 trajectories (50,365 + 9,731), 24 environments, 13 skills, 5 Hz, avg 38 timesteps | [V] |
| S15 | BridgeData V2 paper (arXiv) | https://arxiv.org/abs/2308.12952 | paper | Post-hoc crowdsourced task descriptions; language-conditioned use | [V] |
| S16 | ALOHA project site + ACT code | https://tonyzhaozh.github.io/aloha/ ; https://github.com/tonyzhaozh/act | official | 50 demos/task, 4 tasks 96/84/64/92%, policy inputs images+joints (no language), HDF5 episodes | [V] |
| S17 | ACT paper (arXiv) | https://arxiv.org/abs/2304.13705 | paper | Low-cost bimanual hardware + ACT, imitation from demonstrations | [V] |
| S18 | LIBERO paper (arXiv) | https://arxiv.org/abs/2306.03310 | paper | 130 tasks, 50 demos/task, templated instructions, BERT language encoding | [V] |
| S19 | LIBERO GitHub (README + dataset_utils.py) | https://github.com/Lifelong-Robot-Learning/LIBERO ; https://raw.githubusercontent.com/Lifelong-Robot-Learning/LIBERO/master/libero/libero/utils/dataset_utils.py | official code | 4 suites; `problem_info` HDF5 attr with `language_instruction` (L43–45); HF dataset link | [V] |
| S20 | LeRobot format v3 docs | https://huggingface.co/docs/lerobot/lerobot-dataset-v3 | official docs | `meta/tasks.jsonl`, parquet/MP4 layout, metadata fields | [V] |
| S21 | LeRobot language columns docs (raw) | https://raw.githubusercontent.com/huggingface/lerobot/main/docs/source/language_and_recipes.mdx | official docs | `language_persistent` / `language_events`, row shape, "Most LeRobot datasets ship with a single `task` string" | [V] |
| S22 | LeRobot annotation pipeline docs | https://huggingface.co/docs/lerobot/annotation_pipeline | official docs | `lerobot-annotate` writes NL annotations into parquet language columns | [V] |
| S23 | LeRobot paper (arXiv) | https://arxiv.org/abs/2602.22818 | paper | Open-source end-to-end robot learning library; Feb 2026 | [V] |
| S24 | RLDS README | https://github.com/google-research/rlds | official docs | Episode/step format; mandatory/optional fields; metadata fields | [V] |
| S25 | RxR (ACL Anthology) | https://aclanthology.org/2020.emnlp-main.356/ | paper | Multilingual VLN: English, Hindi, Telugu; word-level alignment | [V] |
| S26 | ReLI (arXiv) | https://arxiv.org/abs/2505.01862 | paper | Cross-lingual language-to-action; 140+ languages; "handful of high-resource languages" | [V] |
| S27 | "When Does Language Matter?" (ACL 2026 + arXiv) | https://aclanthology.org/2026.acl-long.2066/ ; https://arxiv.org/abs/2606.11906 | paper (peer-reviewed) | LIBERO translated to 10 languages; 30–50% success drops; step-wise sensitivity | [V] |
| S28 | "Beyond English" (arXiv) | https://arxiv.org/abs/2606.15714 | paper | Multilingual benchmark translations; "significant multilingual gap" | [V] |
| S29 | "Measuring Language Transfer … Greek" (arXiv + HF) | https://arxiv.org/abs/2609.07470 ; https://huggingface.co/papers/2609.07470 | paper | MT instructions; bilingual +6.7–7.1 pts, ~2/5 English; translator-phrasing overfit; 7 phrasings halves penalty; measurement pitfalls | [V] |
| S30 | Arabic PushT dataset (HF) | https://huggingface.co/datasets/hamzabouajila/ar-pusht-image | community dataset | "first Arabic-translated dataset for VLA"; Arabic instruction string; LeRobot-style `instruction` column; 2.3× claim unverified | [V] card, [U] claim |
| S31 | Sophea Greek LIBERO policy (HF) | https://huggingface.co/ayoubkirouane/Sophea-Nano-Policy-LIBERO-Greek-v1 | community model | Greek/English LIBERO policy; English 62.7%, unseen Greek 27.8%, wrong-instruction control 21.4% | [V] card |
| S32 | AgiBot World Colosseo (arXiv) | https://arxiv.org/abs/2503.06669 | paper | 1M+ trajectories, 217 tasks, five scenarios; language annotations for task and sub-steps; English examples | [V] examples, [U] language |
| S33 | RoboMIND (arXiv) | https://arxiv.org/abs/2412.13877 | paper | 107k trajectories, 479 tasks, 96 object classes, 4 embodiments; "linguistic task descriptions"; language not stated | [V] desc, [U] language |
| S34 | Google Scholar (blocked) | https://scholar.google.com/citations?user=Gbp13nYAAAAJ&hl=en | profile | HTTP 429 on 2026-10-22; no metrics verified | [U] |
| S35 | ALLMAN German VLA (unreachable) | https://www.ias.informatik.tu-darmstadt.de/uploads/Team/MaximilianT%C3%B6lle/AllmAN_A_German_Vision_Language_Action_Model.pdf | lead | PDF fetch timed out twice; do not cite as fact | [U] |

---

## H. Open questions / verify before launch

1. **Language of DROID annotations** — paper never states English; all examples are English. If the site needs the word "English" for DROID, either add "English-language in all published examples" or commission a raw-data check. [U]
2. **Language of RoboMIND and AgiBot World annotations** — not stated. Do not assume English *or* Chinese. [U]
3. **Whether any OXE dataset contains non-English instructions** — we checked index/cards, not raw episodes. The sentence "the OXE index has no language column" is safe; "all OXE instructions are English" is [I] at best. [U/I]
4. **ALLMAN** — a German VLA lead; unreachable. If relevant, find a citable page (arXiv/journal) before use. [U]
5. **Scholar metrics** — blocked (429). Do not display citation counts. [U]
6. **Preprint version/date** — ORCID says 2025-10-04; author page says 10 September 2025; current PDF says 22 October 2026. Use the abstract-page + ORCID links, avoid a single prominent date, or say "2025; current version October 2026". [V]
7. **"Overwhelmingly English" cannot be quantified** from official sources. If the site wants a number, run and publish our own language audit of the seven corpora — that would itself be a differentiator, but it is not this dossier's claim. [U]
8. **LeRobot ecosystem language mix** — not audited. "Most LeRobot datasets ship with a single `task` string" is from the docs; the *language* of those strings is [I]. [U/I]
9. **Any market-size numbers** (e.g. "robotics data market $X") — not researched here; do not invent. If needed, a separate, sourced market note is required. [U]
10. **License review** — before redistributing localized versions, check each dataset's license (LIBERO datasets are CC BY 4.0 per repo table; others vary). Not fully audited here. [U]
11. **Founder bio credentials** — ORCID education (BSc, University of Novi Sad, 1996–2000) and a Google Skills **course badge** (2026) are self-asserted ORCID entries; ORCID's employment section is empty. The badge is not a job: never write "ex-Google" or "Google engineer" (§E.0.1). [V record / credential not independently verified]

---

## Appendix A — Verified multilingual instruction examples already in public artifacts (reference only)

**Why this appendix exists:** to prove that (a) localizations already exist in the wild and (b) real multilingual robot instructions can be sourced. These are **not** Lean Robotics IQ translations, not official dataset releases, and not vetted by an independent reviewer. Do not reuse without a license check. No `sample-translations.md` was created because no *official* dataset card was found that ships official multilingual instructions; these are community artifacts and are logged here instead.

### A.1 Greek ↔ English LIBERO instruction pairs (community model card)

Source: [Sophea-Nano-Policy-LIBERO-Greek-v1 model card](https://huggingface.co/ayoubkirouane/Sophea-Nano-Policy-LIBERO-Greek-v1) (bilingual Greek/English LIBERO policy, base NVIDIA Cosmos3-Nano; card tags link [arXiv:2609.07470](https://arxiv.org/abs/2609.07470)). The English strings are LIBERO task instructions; the Greek strings are the mapping used for training.

| English (LIBERO task string) | Greek (as published on the card) |
|---|---|
| pick up the alphabet soup and put it in the tray | Πιάσε τη σούπα με αλφαβητάκια και βάλε την στο δίσκο |
| put the middle black bowl on the plate | Βάλε το μεσαίο μαύρο μπολ στο πιάτο |
| turn on the stove and put the frying pan on it | Άνοιξε την κουζίνα (εστία) και βάλε το τηγάνι πάνω της |

Card also reports: English instructions 62.7%, Greek phrasings never seen in training 27.8%, wrong-instruction control 21.4% (numbers are the card author's, not independently reproduced).

### A.2 Arabic PushT instruction (community dataset card)

Source: [Arabic PushT Image Dataset card](https://huggingface.co/datasets/hamzabouajila/ar-pusht-image) · data rows via [HF datasets-server first-rows](https://datasets-server.huggingface.co/first-rows?dataset=hamzabouajila%2Far-pusht-image&config=default&split=train).

- Verbatim instruction (task_index 0): **دفع الحجر إلى الهدف**
- Literal gloss (ours, not from the source): "push the block to the target" [I]
- The card claims "The first Arabic-translated dataset for Vision-Language-Action (VLA) robotics" and a 2.3× accuracy improvement for the resulting model. Treat both as the author's claims, unverified [U]. License: Apache-2.0 per card.

### A.3 Multilingual navigation data (no strings fetched)

[RxR](https://aclanthology.org/2020.emnlp-main.356/) ships instructions in English, Hindi and Telugu with word-level time alignment. We did not fetch raw strings (dataset access requirements); cite the languages, not sample text.

**If the site wants an illustrative translation block:** commission native review of 3–5 LIBERO/DROID instructions and label the block "illustrative, reviewed by X on DATE". Do not lift the community examples above into marketing without attribution and a license check.

---

## Appendix B — Safe copy blocks you can adapt

**B.1 Two-sentence positioning (no factual claim beyond §C):**
> Robot-learning data is documented in English. We localize it for the languages your users actually speak — dataset-side, provenance-tracked, and verified before delivery.

**B.2 Method paragraph (bridges the preprint honestly):**
> Lean Robotics IQ is built on two ideas from the founder's preprint: *recursive learning*, which keeps a provenance-linked memory of decisions and consolidates it into future work, and the *Archimedes Principle*, which lets a cheap tier attempt the work first and accepts it only after independent verification. The preprint studies those ideas in an autonomous coding agent (and reports a negative audit result honestly). We apply them to robot-dataset localization: machine translation does the cheap first pass, an independent reviewer adjudicates, and a human escalates the hard cases.

**B.3 Four short lines that are evidence-safe:**
- "One translation is not enough — models learn phrasing variety." (B5)
- "A gate is only as good as its verifier." (Prop. 6.11, labelled "from the preprint")
- "We keep the original strings, the reviewer, and the back-translation." (service promise)
- "We publish what we changed and what we could not verify." (service promise, on-brand with §7.7)

**B.4 Footer citation block:** use the citation in §E.0 plus links to ORCID and the abstract page. Do not add a DOI.

---

*End of dossier. Refresh instructions and the confidence legend live in `content/README.md`.*

# Post-processing added by the Lead (beyond image-forge's delivery)

Two small edits were made to the shipped plates after the art set was delivered:

1. **`lang.jpg` — dark side bars removed.** The treated plate retained the source render's
   near-black vertical borders (≈22 px left, ≈17 px right) which read as letterboxing at page
   size. Cropped to the content field and resampled to the required 1000×1000:

   ```bash
   python3 - <<'PY'
   from PIL import Image
   im = Image.open('assets/img/art/lang.jpg')
   box = (22, 19, 983, 981)   # left, top, right, bottom — removes the bars
   im.crop(box).resize((1000, 1000), Image.LANCZOS).save(
       'assets/img/art/lang.jpg', quality=84, optimize=True, progressive=True)
   PY
   ```

   Final bytes depend on the JPEG re-encode (roughly 138 KB; the delivered original was 127,926 bytes). Everything else in the plate is unchanged.

2. **Branded social card — `assets/img/og-card.jpg` (1200×630).** The meta/twitter tags use this
   composed card, not the text-free `og.jpg` plate. It is a headless-Chrome render of a small HTML
   layout (`1200×630`) that pairs the brand mark and headline with `hero-atlas.jpg` on the right.
   Source HTML and the Playwright script are tracked in the repository:
   [`tools/og-card.html`](../../../tools/og-card.html) and
   [`tools/make-og.js`](../../../tools/make-og.js). Rebuild with a local server running, then
   `NODE_PATH=/home/dusan/.local/lib/node_modules node tools/make-og.js`.
   The plate itself (`og.jpg`, orange re-release of the hero corridor) remains in the set as an
   alternative social base.

The model/prompt/seed provenance for the underlying plates is unchanged and recorded in
`manifest.json` and `PROCESS.md`.

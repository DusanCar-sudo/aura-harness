// Archimedes site: animated gate loop + seeded simulator. No dependencies, no network.
(() => {
  'use strict';

  // ---- shared math (paper: q = pρ + (1−p)α, E[C]/c_L = c_S/c_L + (1 − q)) ----
  const acceptProb = (p, rho, alpha) => p * rho + (1 - p) * alpha;
  const costPerAttempt = (r, q) => r + (1 - q);
  console.assert(acceptProb(1, 1, 0) === 1 && acceptProb(0, 1, 0) === 0, 'q formula');
  console.assert(Math.abs(costPerAttempt(0.1, 1) - 0.1) < 1e-12 && costPerAttempt(0.1, 0) === 1.1, 'cost formula');

  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  // ---- 1. live loop diagram ----
  const flow = document.querySelector('[data-flow]');
  if (flow && !reduceMotion) {
    const NS = 'http://www.w3.org/2000/svg';
    const layer = flow.querySelector('[data-tokens]');
    const path = (id) => document.getElementById(id);
    const SPEED = 170; // svg units per second
    const SPAWN_MS = 950;
    const live = [];
    let lastSpawn = 0, lastT = 0, visible = true;

    const makeToken = (cls, r, route) => {
      const el = document.createElementNS(NS, 'circle');
      el.setAttribute('r', r);
      el.setAttribute('class', cls);
      layer.appendChild(el);
      live.push({ el, route, seg: 0, dist: 0, entered: false });
    };

    // illustrative rates only: 75% of tasks get an attempt, p=.7, ρ=.9, α=.05
    const spawnTask = () => {
      const lesson = () => makeToken('token--lesson', 4, [['w-lesson', 'token--lesson'], ['w-learn', 'token--lesson']]);
      const head = [['w-in', 'token--skip'], ['w-try', 'token--skip'], ['w-check', 'token--skip']];
      let route;
      if (Math.random() >= 0.75) {
        route = [['w-in', 'token--skip'], ['w-skip', 'token--skip'], ['w-large-ans', 'token--skip']];
      } else {
        const correct = Math.random() < 0.7;
        const accepted = correct ? Math.random() < 0.9 : Math.random() < 0.05;
        route = accepted
          ? [...head, ['w-accept', correct ? 'token--accept' : 'token--leak']]
          : [...head, ['w-reject', 'token--reject'], ['w-large-ans', 'token--reject', lesson]];
      }
      makeToken(route[0][1], 6, route);
    };

    const step = (now) => {
      const dt = lastT ? Math.min((now - lastT) / 1000, 0.05) : 0;
      lastT = now;
      if (visible && now - lastSpawn > SPAWN_MS) { lastSpawn = now; spawnTask(); }
      for (let i = live.length - 1; i >= 0; i--) {
        const tk = live[i];
        const [id, cls, onEnter] = tk.route[tk.seg];
        if (!tk.entered) {
          tk.entered = true;
          tk.el.setAttribute('class', cls);
          if (onEnter) onEnter(); // the lesson splits off as the large model answers
        }
        const p = path(id);
        tk.dist += SPEED * dt;
        if (tk.dist >= p.getTotalLength()) {
          tk.seg += 1; tk.dist = 0; tk.entered = false;
          if (tk.seg >= tk.route.length) { tk.el.remove(); live.splice(i, 1); }
          continue;
        }
        const pt = p.getPointAtLength(tk.dist);
        tk.el.setAttribute('cx', pt.x);
        tk.el.setAttribute('cy', pt.y);
      }
      requestAnimationFrame(step);
    };

    if ('IntersectionObserver' in window) {
      new IntersectionObserver(([e]) => { visible = e.isIntersecting; }).observe(flow);
    }
    requestAnimationFrame(step);
  }

  // ---- 2. simulator ----
  const form = document.querySelector('[data-sim]');
  if (!form) return;

  const N = 400, M0 = 5, WINDOW = 40, SEED = 20261022;
  // mulberry32: tiny seeded PRNG so the same sliders always give the same run
  const rng = (seed) => () => {
    seed = (seed + 0x6D2B79F5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };

  const simulate = ({ p, rho, alpha, r, eps }) => {
    const rand = rng(SEED);
    let n = 0, passes = 0, cost = 0;
    const out = { acc: 0, leak: 0, rej: 0, skip: 0, attempts: 0, costSeries: [], attemptSeries: [] };
    const recent = [];
    for (let i = 0; i < N; i++) {
      const trusted = n >= M0 && passes / n > r;
      // draw all three numbers every task so one slider doesn't reshuffle the others
      const uProbe = rand(), uCorrect = rand(), uVerdict = rand();
      const attempt = trusted || uProbe < eps;
      if (!attempt) {
        out.skip++; cost += 1;
      } else {
        out.attempts++; n++; cost += r;
        const correct = uCorrect < p;
        const accepted = correct ? uVerdict < rho : uVerdict < alpha;
        if (accepted) { passes++; if (correct) out.acc++; else out.leak++; }
        else { out.rej++; cost += 1; }
      }
      recent.push(attempt ? 1 : 0);
      if (recent.length > WINDOW) recent.shift();
      out.costSeries.push(cost / (i + 1));
      out.attemptSeries.push(recent.reduce((a, b) => a + b, 0) / recent.length);
    }
    out.cost = cost / N;
    return out;
  };

  // self-check: perfect small model + perfect verifier costs exactly r; ε = 0 never opens (the audit's cold start)
  {
    const s = simulate({ p: 1, rho: 1, alpha: 0, r: 0.1, eps: 1 });
    console.assert(s.acc === N && s.leak === 0 && Math.abs(s.cost - 0.1) < 1e-9, 'simulator: perfect case');
    const z = simulate({ p: 0.9, rho: 0.9, alpha: 0.05, r: 0.1, eps: 0 });
    console.assert(z.attempts === 0 && z.cost === 1, 'simulator: cold start');
  }

  const root = form.parentElement;
  const $ = (sel) => root.querySelector(sel);
  const pct = (x) => `${(x * 100).toFixed(x > 0 && x < 0.1 ? 1 : 0)}%`;
  const yOf = (v) => 170 - Math.min(v, 1.5) * (160 / 1.5);
  const xOf = (i) => 40 + (i / (N - 1)) * 550;
  const line = (series) => series.map((v, i) => `${i ? 'L' : 'M'}${xOf(i).toFixed(1)} ${yOf(v).toFixed(1)}`).join('');

  const render = () => {
    const v = Object.fromEntries([...new FormData(form)].map(([k, x]) => [k, Number(x)]));
    form.querySelectorAll('[data-out]').forEach((o) => { o.textContent = v[o.dataset.out].toFixed(2); });

    const s = simulate(v);
    ['acc', 'leak', 'rej', 'skip'].forEach((k) => {
      $(`[data-bar="${k}"]`).style.width = `${(s[k] / N) * 100}%`;
      $(`[data-count="${k}"]`).textContent = s[k];
    });
    $('[data-kpi="cost"]').textContent = pct(s.cost);
    $('[data-kpi="pairs"]').textContent = s.rej;
    $('[data-kpi="leak"]').textContent = pct(s.leak / N);
    $('[data-line="cost"]').setAttribute('d', line(s.costSeries));
    $('[data-line="attempt"]').setAttribute('d', line(s.attemptSeries));

    const q = acceptProb(v.p, v.rho, v.alpha);
    let msg, bad = true;
    if (s.attempts === 0) {
      msg = 'Stalled. The small model never got a single try, so the gate never collected a verdict. This is what our audit found (0 of 153). Raise ε.';
    } else if (q > v.r) {
      msg = `Pays off: acceptance q = ${q.toFixed(2)} beats the cost ratio ${v.r.toFixed(2)}. An attempt costs ${pct(costPerAttempt(v.r, q))} of a large call on average.`;
      bad = false;
    } else {
      msg = `Doesn't pay here: q = ${q.toFixed(2)} is not above the cost ratio ${v.r.toFixed(2)}. The gate keeps this kind of task on the large model, and the probes still collect lessons.`;
    }
    const verdict = $('[data-verdict]');
    verdict.textContent = msg;
    verdict.classList.toggle('is-bad', bad);
    $('[data-analytic]').textContent = `The small model is not retrained during this run, so this shows the gate alone. Wrong answers shipped stay near (1 − p)·α = ${pct((1 - v.p) * v.alpha)} of attempts.`;
  };

  form.addEventListener('input', render);
  render();
})();

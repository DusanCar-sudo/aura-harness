/* ==========================================================================
   Lean Robotics IQ — site.js
   Vanilla ES2018, no dependencies. Everything is progressive:
   without JS the pages still read, the tabs still show the first panel,
   and every figure is rendered server-side.
   ========================================================================== */
(function () {
  "use strict";

  var $ = function (sel, root) { return (root || document).querySelector(sel); };
  var $$ = function (sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); };
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var fmtInt = new Intl.NumberFormat("en-US");
  var fmtDec = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 });

  /* ---------------------------------------------------------- header + progress */
  var header = $("[data-header]");
  var line = $(".scrollline span");
  var ticking = false;

  function onScroll() {
    if (ticking) return;
    ticking = true;
    window.requestAnimationFrame(function () {
      var y = window.scrollY || window.pageYOffset;
      var max = Math.max(1, document.documentElement.scrollHeight - window.innerHeight);
      if (line) line.style.width = Math.min(100, (y / max) * 100) + "%";
      if (header) header.classList.toggle("is-scrolled", y > 8);
      ticking = false;
    });
  }
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  /* ---------------------------------------------------------- mobile nav */
  var burger = $("[data-burger]");
  var mobile = $("[data-mobilenav]");
  var lastFocus = null;

  function closeMenu(restore) {
    if (!burger || !mobile || mobile.hidden) return;
    burger.setAttribute("aria-expanded", "false");
    mobile.classList.remove("is-open");
    document.body.classList.remove("menu-open");
    window.setTimeout(function () { if (!mobile.classList.contains("is-open")) mobile.hidden = true; }, 260);
    if (restore && lastFocus) lastFocus.focus();
  }
  function openMenu() {
    if (!burger || !mobile) return;
    lastFocus = document.activeElement;
    mobile.hidden = false;
    mobile.classList.add("is-open");
    document.body.classList.add("menu-open");
    burger.setAttribute("aria-expanded", "true");
    var first = mobile.querySelector("a");
    if (first) first.focus();
  }
  if (burger && mobile) {
    burger.addEventListener("click", function () {
      if (burger.getAttribute("aria-expanded") === "true") closeMenu(true);
      else openMenu();
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape") closeMenu(true);
    });
    $$("a", mobile).forEach(function (a) { a.addEventListener("click", function () { closeMenu(false); }); });
    window.addEventListener("resize", function () {
      if (window.innerWidth > 1120) closeMenu(false);
    });
  }

  /* ---------------------------------------------------------- reveal on scroll */
  var reveals = $$(".reveal");
  if (reveals.length) {
    if (reduced || !("IntersectionObserver" in window)) {
      reveals.forEach(function (el) { el.classList.add("in"); });
    } else {
      var ro = new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) {
          if (!entry.isIntersecting) return;
          entry.target.classList.add("in");
          ro.unobserve(entry.target);
        });
      }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 });
      reveals.forEach(function (el) { ro.observe(el); });
    }
  }

  /* ---------------------------------------------------------- count-up numbers */
  function animateCount(el) {
    var target = parseFloat(el.getAttribute("data-count"));
    if (isNaN(target)) return;
    var decimals = (el.getAttribute("data-decimals") || "0") | 0;
    var prefix = el.getAttribute("data-prefix") || "";
    var suffix = el.getAttribute("data-suffix") || "";
    var dur = reduced ? 0 : 1300;
    var start = null;
    function frame(ts) {
      if (start === null) start = ts;
      var p = dur === 0 ? 1 : Math.min(1, (ts - start) / dur);
      var eased = 1 - Math.pow(1 - p, 3);
      var val = target * eased;
      el.textContent = prefix + (decimals ? fmtDec.format(val) : fmtInt.format(Math.round(val))) + suffix;
      if (p < 1) window.requestAnimationFrame(frame);
    }
    window.requestAnimationFrame(frame);
  }
  var counters = $$("[data-count]");
  if (counters.length) {
    if (reduced || !("IntersectionObserver" in window)) {
      counters.forEach(function (el) {
        var t = parseFloat(el.getAttribute("data-count"));
        var d = (el.getAttribute("data-decimals") || "0") | 0;
        el.textContent = (el.getAttribute("data-prefix") || "") +
          (d ? fmtDec.format(t) : fmtInt.format(t)) + (el.getAttribute("data-suffix") || "");
      });
    } else {
      var co = new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) {
          if (!entry.isIntersecting) return;
          animateCount(entry.target);
          co.unobserve(entry.target);
        });
      }, { threshold: 0.5 });
      counters.forEach(function (el) { co.observe(el); });
    }
  }

  /* ---------------------------------------------------------- language tabs */
  $$("[data-tabs]").forEach(function (group) {
    var tabs = $$('[role="tab"]', group);
    var panels = tabs.map(function (t) { return document.getElementById(t.getAttribute("aria-controls")); }).filter(Boolean);
    function select(index, focus) {
      tabs.forEach(function (tab, i) {
        var on = i === index;
        tab.setAttribute("aria-selected", on ? "true" : "false");
        tab.tabIndex = on ? 0 : -1;
        if (panels[i]) panels[i].hidden = !on;
      });
      if (focus && tabs[index]) tabs[index].focus();
    }
    tabs.forEach(function (tab, i) {
      tab.addEventListener("click", function () { select(i, false); });
      tab.addEventListener("keydown", function (e) {
        var key = e.key;
        var next = null;
        if (key === "ArrowRight" || key === "ArrowDown") next = (i + 1) % tabs.length;
        else if (key === "ArrowLeft" || key === "ArrowUp") next = (i - 1 + tabs.length) % tabs.length;
        else if (key === "Home") next = 0;
        else if (key === "End") next = tabs.length - 1;
        if (next !== null) { e.preventDefault(); select(next, true); }
      });
    });
    if (tabs.length) select(0, false);
  });

  /* ---------------------------------------------------------- loop diagram */
  var loopcards = $$("[data-loop-card]");
  loopcards.forEach(function (card) {
    var nodes = $$("[data-loop]", card);
    var buttons = $$(".loop__btn", card);
    var detail = $(".loopcard__detail", card);
    var title = $("[data-loop-title]", card);
    var timer = null;
    var stages = buttons.length;

    function show(index, fromUser) {
      nodes.forEach(function (n) {
        var on = parseInt(n.getAttribute("data-loop"), 10) === index;
        n.classList.toggle("is-on", on);
        if (n.getAttribute("role") === "button") n.setAttribute("aria-pressed", on ? "true" : "false");
      });
      buttons.forEach(function (b) {
        var on = parseInt(b.getAttribute("data-loop"), 10) === index;
        b.setAttribute("aria-pressed", on ? "true" : "false");
      });
      var meta = buttons[index - 1];
      if (meta) {
        if (detail) {
          var head = meta.getAttribute("data-detail-title") || meta.getAttribute("data-name") || "";
          var text = meta.getAttribute("data-detail") || "";
          detail.innerHTML = head ? "<b>" + head + "</b>" + text : text;
        }
        if (title) title.textContent = meta.getAttribute("data-name") || "";
      }
      if (fromUser && timer) { window.clearInterval(timer); timer = null; }
    }

    nodes.forEach(function (n) {
      var idx = parseInt(n.getAttribute("data-loop"), 10);
      n.addEventListener("click", function () { show(idx, true); });
      n.addEventListener("keydown", function (e) {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); show(idx, true); }
      });
    });
    buttons.forEach(function (b) {
      b.addEventListener("click", function () { show(parseInt(b.getAttribute("data-loop"), 10), true); });
    });

    show(1, false);
    if (!reduced && stages > 1) {
      var current = 1;
      timer = window.setInterval(function () {
        current = (current % stages) + 1;
        show(current, false);
      }, 3400);
      var stop = function () { if (timer) { window.clearInterval(timer); timer = null; } };
      card.addEventListener("mouseenter", stop, { once: true });
      card.addEventListener("focusin", stop, { once: true });
      document.addEventListener("visibilitychange", function (e) {
        if (document.hidden) stop();
      });
    }
  });

  /* ---------------------------------------------------------- break-even bench */
  var benches = $$("[data-bench]");
  benches.forEach(function (root) {
    var qIn = $("[data-bench-q]", root);
    var rIn = $("[data-bench-r]", root);
    var gateFill = $("[data-bench-gate]", root);
    var largeFill = $("[data-bench-large]", root);
    var gateVal = $("[data-bench-gate-val]", root);
    var largeVal = $("[data-bench-large-val]", root);
    var savingEl = $("[data-bench-saving]", root);
    var savingLbl = $("[data-bench-saving-label]", root);
    var verdict = $("[data-bench-verdict]", root);
    var qOut = $("[data-bench-q-out]", root);
    var rOut = $("[data-bench-r-out]", root);
    if (!qIn || !rIn) return;

    function update() {
      var q = Math.max(0, Math.min(100, parseFloat(qIn.value))) / 100;
      var r = Math.max(0, Math.min(50, parseFloat(rIn.value))) / 100;
      var gate = r + (1 - q);
      var large = 1;
      var max = Math.max(gate, large);
      if (gateFill) gateFill.style.width = (gate / max) * 100 + "%";
      if (largeFill) largeFill.style.width = (large / max) * 100 + "%";
      if (gateVal) gateVal.textContent = Math.round(gate * 100) + "%";
      if (largeVal) largeVal.textContent = "100%";
      if (qOut) qOut.textContent = Math.round(q * 100) + "%";
      if (rOut) rOut.textContent = (Math.round(r * 1000) / 10) + "%";
      var saving = q - r;
      if (savingEl) {
        savingEl.textContent = (saving >= 0 ? "" : "+") + Math.abs(Math.round(saving * 100)) + "%";
        savingEl.classList.toggle("bench__saving--off", saving < 0);
      }
      if (savingLbl) savingLbl.textContent = saving >= 0 ? "saved per task vs. always-large" : "extra cost per task vs. always-large";
      if (verdict) {
        var open = saving > 0;
        verdict.classList.toggle("bench__verdict--off", !open);
        verdict.innerHTML = open
          ? "<b>Gate admissible.</b> The verified acceptance rate (" + Math.round(q * 100) + "%) clears the cost ratio (" + (Math.round(r * 1000) / 10) + "%), so the cheap tier may carry the task. Expected cost is " + Math.round(gate * 100) + "% of always-large."
          : "<b>Gate stays shut.</b> Acceptance (" + Math.round(q * 100) + "%) does not exceed the cost ratio (" + (Math.round(r * 1000) / 10) + "%), so routing cheap would cost " + Math.round(gate * 100) + "% of always-large. The inequality says no.";
      }
    }
    [qIn, rIn].forEach(function (el) {
      el.addEventListener("input", update);
      el.addEventListener("change", update);
    });
    update();
  });

  /* ---------------------------------------------------------- safety envelope */
  $$("[data-envelope]").forEach(function (root) {
    var pIn = $("[data-env-p]", root);
    var pLIn = $("[data-env-pl]", root);
    var aIn = $("[data-env-alpha]", root);
    var pOut = $("[data-env-p-out]", root);
    var pLOut = $("[data-env-pl-out]", root);
    var aOut = $("[data-env-alpha-out]", root);
    var boundOuts = $$("[data-env-bound]", root);
    var note = $("[data-env-note]", root);
    if (!pIn || !pLIn || !aIn) return;

    function update() {
      var p = Math.max(0, Math.min(100, parseFloat(pIn.value))) / 100;
      var pL = Math.max(0, Math.min(100, parseFloat(pLIn.value))) / 100;
      var a = Math.max(0, Math.min(30, parseFloat(aIn.value))) / 100;
      var bound = (1 - p) * a + (1 - pL);
      var baseline = 1 - pL;
      if (pOut) pOut.textContent = Math.round(p * 100) + "%";
      if (pLOut) pLOut.textContent = Math.round(pL * 100) + "%";
      if (aOut) aOut.textContent = (Math.round(a * 1000) / 10) + "%";
      boundOuts.forEach(function (el) { el.textContent = (Math.round(bound * 1000) / 10) + "%"; });
      if (note) {
        var excess = bound - baseline;
        note.innerHTML = pL < p
          ? "<b>The large tier is not better here.</b> The formula still bounds the error, but alternation can only help quality when pL ≥ p. Fix the tier premise before reading the number."
          : "<b>Excess error over always-large: " + (Math.round(excess * 1000) / 10) + " percentage points.</b> That excess is exactly (1−p)·α: it comes from the producer, admitted by the verifier. A perfect verifier (α = 0) makes the bound equal to always-large.";
      }
    }
    [pIn, pLIn, aIn].forEach(function (el) {
      el.addEventListener("input", update);
      el.addEventListener("change", update);
    });
    update();
  });

  /* ---------------------------------------------------------- method-page toc scrollspy */
  var tocLinks = $$(".doc__toc a");
  if (tocLinks.length && "IntersectionObserver" in window) {
    var byId = {};
    tocLinks.forEach(function (a) {
      var id = (a.getAttribute("href") || "").replace("#", "");
      if (id) byId[id] = a;
    });
    var spy = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (!entry.isIntersecting) return;
        var id = entry.target.id;
        tocLinks.forEach(function (a) { a.classList.remove("is-active"); });
        if (byId[id]) byId[id].classList.add("is-active");
      });
    }, { rootMargin: "-18% 0px -66% 0px" });
    Object.keys(byId).forEach(function (id) {
      var el = document.getElementById(id);
      if (el) spy.observe(el);
    });
  }

  /* ---------------------------------------------------------- copy email */
  $$("[data-copy]").forEach(function (btn) {
    var label = btn.querySelector("[data-copy-label]") || btn;
    var original = label.textContent;
    btn.addEventListener("click", function () {
      var text = btn.getAttribute("data-copy");
      function done() {
        btn.classList.add("is-done");
        label.textContent = "Copied — " + text;
        window.setTimeout(function () {
          btn.classList.remove("is-done");
          label.textContent = original;
        }, 2400);
      }
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, function () { window.location.href = "mailto:" + text; });
      } else {
        window.location.href = "mailto:" + text;
      }
    });
  });

  /* ---------------------------------------------------------- table row highlight */
  $$("[data-table] tbody tr").forEach(function (row) {
    row.addEventListener("mouseenter", function () { row.classList.add("is-hot"); });
    row.addEventListener("mouseleave", function () { row.classList.remove("is-hot"); });
  });
})();

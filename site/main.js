// Sayvane landing: small, dependency free progressive enhancements.
(() => {
  const root = document.documentElement;
  root.classList.add("js");
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // OS aware download label: point visitors at their own platform.
  const ua = navigator.userAgent;
  const os = /Windows/i.test(ua) ? "windows" : /Mac OS X|Macintosh/i.test(ua) && !/iPhone|iPad/i.test(ua) ? "mac" : null;
  if (os) {
    const label = os === "mac" ? "Download for Mac" : "Download for Windows";
    document.querySelectorAll("[data-os-label]").forEach((el) => (el.textContent = label));
    document.querySelector(`[data-platform="${os}"]`)?.classList.add("is-yours");
  }

  // Mobile menu with staggered link entrance (60ms steps, capped at 400ms).
  const toggle = document.querySelector(".menu-toggle");
  const menu = document.getElementById("mobile-menu");
  const setMenu = (open) => {
    toggle.setAttribute("aria-expanded", String(open));
    toggle.setAttribute("aria-label", open ? "Close menu" : "Open menu");
    document.body.style.overflow = open ? "hidden" : "";
    if (open) {
      menu.hidden = false;
      menu.querySelectorAll("li").forEach((li, i) => (li.style.transitionDelay = `${Math.min(i * 60, 400)}ms`));
      requestAnimationFrame(() => requestAnimationFrame(() => menu.classList.add("is-open")));
    } else {
      menu.classList.remove("is-open");
      menu.hidden = true;
    }
  };
  toggle?.addEventListener("click", () => setMenu(toggle.getAttribute("aria-expanded") !== "true"));
  menu?.addEventListener("click", (e) => e.target.closest("a") && setMenu(false));
  addEventListener("keydown", (e) => e.key === "Escape" && toggle?.getAttribute("aria-expanded") === "true" && setMenu(false));

  // Scroll reveals, once per element.
  const io = new IntersectionObserver(
    (entries) => entries.forEach((en) => {
      if (en.isIntersecting) {
        en.target.classList.add("in");
        io.unobserve(en.target);
      }
    }),
    { rootMargin: "0px 0px -10% 0px", threshold: 0.12 },
  );
  document.querySelectorAll(".reveal").forEach((el) => io.observe(el));

  // Current section in the nav.
  const navLinks = [...document.querySelectorAll(".nav-links a")];
  const navIo = new IntersectionObserver(
    (entries) => entries.forEach((en) => {
      if (!en.isIntersecting) return;
      navLinks.forEach((a) => a.classList.toggle("is-active", a.getAttribute("href") === `#${en.target.id}`));
    }),
    { rootMargin: "-45% 0px -50% 0px" },
  );
  navLinks.forEach((a) => {
    const target = document.querySelector(a.getAttribute("href"));
    if (target) navIo.observe(target);
  });

  // Tagline: words light up one at a time as they cross a trigger line.
  const tagline = document.querySelector("[data-words]");
  if (tagline) {
    const highlight = new Set(["personal.", "nothing"]);
    const words = tagline.textContent.trim().split(/\s+/);
    tagline.innerHTML = words
      .map((w) => `<span class="w${highlight.has(w) ? " hl" : ""}">${w}</span>`)
      .join(" ");
    const spans = [...tagline.querySelectorAll(".w")];
    if (reduceMotion) {
      spans.forEach((s) => s.classList.add("on"));
    } else {
      let ticking = false;
      const update = () => {
        ticking = false;
        const r = tagline.getBoundingClientRect();
        const trigger = innerHeight * 0.72;
        const progress = (trigger - r.top) / (r.height + innerHeight * 0.25);
        const lit = Math.round(Math.max(0, Math.min(1, progress)) * spans.length);
        spans.forEach((s, i) => s.classList.toggle("on", i < lit));
      };
      addEventListener("scroll", () => {
        if (!ticking) {
          ticking = true;
          requestAnimationFrame(update);
        }
      }, { passive: true });
      update();
    }
  }

  // Tour tabs (roving tabindex, arrow keys).
  const tabs = [...document.querySelectorAll('[role="tab"]')];
  const select = (tab) => {
    tabs.forEach((t) => {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
    });
  };
  tabs.forEach((t, i) => {
    t.addEventListener("click", () => select(t));
    t.addEventListener("keydown", (e) => {
      const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!d) return;
      const next = tabs[(i + d + tabs.length) % tabs.length];
      next.focus();
      select(next);
    });
  });

  // Mini waveform in the feature card.
  const mini = document.querySelector("[data-mini-wave]");
  if (mini) {
    mini.innerHTML = Array.from({ length: 9 }, (_, i) => `<i style="animation-delay:${(i * 137) % 600}ms"></i>`).join("");
  }

  // Hero demo: the floating bar listens, then words land in the field.
  const wave = document.querySelector("[data-wave]");
  const pill = document.querySelector("[data-pill]");
  const pillLabel = document.querySelector("[data-pill-label]");
  const textEl = document.querySelector("[data-demo-text]");
  const appEl = document.querySelector("[data-demo-app]");
  const toEl = document.querySelector("[data-demo-to]");
  if (!wave || !textEl) return;

  const BARS = 14;
  wave.innerHTML = "<i></i>".repeat(BARS);
  const bars = [...wave.children];

  const scenes = [
    { app: "Slack", to: "#design", text: "Love the new onboarding flow. Can we move the permissions step after the first note, so people hear it work before we ask for anything?" },
    { app: "Mail", to: "To: Priya Raman", text: "Hi Priya, thank you for staying late on Thursday. The launch went smoothly because of you, and I wanted you to hear that from me directly." },
    { app: "Notes", to: "Weekend", text: "Repot the fig tree, finish the second half of the novel, and call Grandma about the trip in December." },
  ];

  if (reduceMotion) {
    appEl.textContent = scenes[0].app;
    toEl.textContent = scenes[0].to;
    textEl.textContent = scenes[0].text;
    bars.forEach((b, i) => (b.style.transform = `scaleY(${0.3 + 0.5 * Math.abs(Math.sin(i))})`));
    pillLabel.textContent = "Inserted";
    return;
  }

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  let mode = "idle";
  let t = 0;
  const animateWave = () => {
    t += 0.12;
    bars.forEach((b, i) => {
      let v = 0.22;
      if (mode === "listen") {
        const env = 0.55 + 0.45 * Math.sin(t * 0.7 + i * 0.3);
        v = 0.18 + env * Math.abs(Math.sin(t * 2.1 + i * 0.9) * Math.cos(t * 1.3 + i * 0.4));
      } else if (mode === "busy") {
        v = 0.2 + 0.25 * Math.max(0, Math.sin(t * 1.6 - i * 0.5));
      }
      b.style.transform = `scaleY(${v.toFixed(3)})`;
    });
    requestAnimationFrame(animateWave);
  };
  requestAnimationFrame(animateWave);

  const setMode = (m, label) => {
    mode = m;
    pillLabel.textContent = label;
    pill.classList.toggle("is-idle", m === "idle");
    pill.classList.toggle("is-busy", m === "busy");
  };

  // Only run while visible, to save battery.
  let visible = true;
  new IntersectionObserver(([en]) => (visible = en.isIntersecting)).observe(textEl.closest(".stage"));
  const waitVisible = async () => { while (!visible) await sleep(300); };

  (async function loop() {
    for (let n = 0; ; n = (n + 1) % scenes.length) {
      const s = scenes[n];
      await waitVisible();
      appEl.textContent = s.app;
      toEl.textContent = s.to;
      textEl.textContent = "";
      setMode("idle", "Hold fn");
      await sleep(900);
      setMode("listen", "Listening");
      await sleep(2600);
      setMode("busy", "Transcribing");
      await sleep(900);
      setMode("busy", "Typing");
      const words = s.text.split(" ");
      for (let i = 0; i < words.length; i++) {
        textEl.textContent += (i ? " " : "") + words[i];
        await sleep(55 + Math.random() * 45);
      }
      setMode("idle", `Inserted in ${s.app}`);
      await sleep(3200);
    }
  })();
})();

// Sayvane landing: dependency free motion and interaction.
(() => {
  const root = document.documentElement;
  root.classList.add("js");
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const finePointer = matchMedia("(pointer: fine)").matches;
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const map = (v, a, b) => clamp((v - a) / (b - a));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  // ---------- OS aware download label ----------
  const ua = navigator.userAgent;
  const os = /Windows/i.test(ua) ? "windows" : /Mac OS X|Macintosh/i.test(ua) && !/iPhone|iPad/i.test(ua) ? "mac" : null;
  if (os) {
    const label = os === "mac" ? "Download for Mac" : "Download for Windows";
    document.querySelectorAll("[data-os-label]").forEach((el) => (el.textContent = label));
    document.querySelector(`[data-platform="${os}"]`)?.classList.add("is-yours");
  }
  // Main buttons download the right installer directly when the platform is certain;
  // otherwise they scroll to the download cards.
  const setDirect = (href) => document.querySelectorAll("[data-os-cta]").forEach((a) => (a.href = href));
  if (os === "windows") {
    // Only Chromium based browsers reveal the CPU; otherwise the cards let people choose.
    winArch().then((arch) => {
      if (arch === "arm") setDirect("/download/windows-arm");
      else if (arch === "x64") setDirect("/download/windows");
    });
  }
  if (os === "mac") {
    macArch().then((arch) => {
      if (arch === "arm") setDirect("/download/mac-arm");
      else if (arch === "intel") setDirect("/download/mac-intel");
    });
  }
  async function winArch() {
    try {
      const hints = await navigator.userAgentData?.getHighEntropyValues?.(["architecture"]);
      if (hints?.architecture === "arm") return "arm";
      if (hints?.architecture === "x86") return "x64";
    } catch {}
    return null;
  }
  async function macArch() {
    try {
      const hints = await navigator.userAgentData?.getHighEntropyValues?.(["architecture"]);
      if (hints?.architecture === "arm") return "arm";
      if (hints?.architecture === "x86") return "intel";
    } catch {}
    try {
      const gl = document.createElement("canvas").getContext("webgl");
      const ext = gl?.getExtension("WEBGL_debug_renderer_info");
      const gpu = ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "";
      if (/Apple M\d/i.test(gpu)) return "arm";
      if (/Intel|AMD|Radeon/i.test(gpu)) return "intel";
    } catch {}
    return null;
  }

  // ---------- Mobile menu ----------
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

  // ---------- Hero title: words rise out of blur ----------
  const title = document.querySelector("[data-split]");
  if (title) {
    const wrap = (node) => {
      [...node.childNodes].forEach((n) => {
        if (n.nodeType === 3) {
          const frag = document.createDocumentFragment();
          n.textContent.split(/(\s+)/).forEach((part) => {
            if (!part) return;
            if (/^\s+$/.test(part)) frag.append(" ");
            else {
              const s = document.createElement("span");
              s.className = "split-word";
              s.textContent = part;
              frag.append(s);
            }
          });
          n.replaceWith(frag);
        } else if (n.nodeType === 1) wrap(n);
      });
    };
    wrap(title);
    title.querySelectorAll(".split-word").forEach((w, i) => {
      w.style.transitionDelay = `${150 + i * 90}ms`;
      requestAnimationFrame(() => requestAnimationFrame(() => w.classList.add("in")));
    });
  }

  // ---------- Scroll reveals ----------
  const io = new IntersectionObserver(
    (entries) => entries.forEach((en) => {
      if (en.isIntersecting) {
        en.target.classList.add("in");
        io.unobserve(en.target);
      }
    }),
    { rootMargin: "0px 0px -8% 0px", threshold: 0.1 },
  );
  document.querySelectorAll(".reveal").forEach((el) => {
    // Stagger siblings that enter together.
    const sibs = [...el.parentElement.children].filter((c) => c.classList.contains("reveal"));
    el.style.setProperty("--d", `${Math.min(sibs.indexOf(el), 5) * 90}ms`);
    io.observe(el);
  });
  // Hero copy reveals right away.
  document.querySelectorAll(".hero .reveal").forEach((el, i) => {
    el.style.setProperty("--d", `${700 + i * 140}ms`);
    requestAnimationFrame(() => el.classList.add("in"));
  });

  // ---------- Nav: current section, hide on scroll down ----------
  const navWrap = document.querySelector(".nav-wrap");
  const navLinks = [...document.querySelectorAll(".nav-links a")];
  const navIo = new IntersectionObserver(
    (entries) => entries.forEach((en) => {
      if (!en.isIntersecting) return;
      navLinks.forEach((a) => a.classList.toggle("is-active", a.getAttribute("href") === `#${en.target.id}`));
    }),
    { rootMargin: "-45% 0px -50% 0px" },
  );
  navLinks.forEach((a) => {
    const t = document.querySelector(a.getAttribute("href"));
    if (t) navIo.observe(t);
  });

  // ---------- Pointer: glow, spotlight cards, magnetic buttons ----------
  if (finePointer && !reduceMotion) {
    const glow = document.querySelector(".cursor-glow");
    let gx = innerWidth / 2, gy = innerHeight / 2, tx = gx, ty = gy;
    addEventListener("pointermove", (e) => {
      tx = e.clientX; ty = e.clientY;
      root.classList.add("has-pointer");
    }, { passive: true });
    (function follow() {
      gx += (tx - gx) * 0.12; gy += (ty - gy) * 0.12;
      glow.style.transform = `translate3d(${gx}px, ${gy}px, 0)`;
      requestAnimationFrame(follow);
    })();

    document.querySelectorAll(".spot").forEach((el) => {
      el.addEventListener("pointermove", (e) => {
        const r = el.getBoundingClientRect();
        el.style.setProperty("--mx", `${e.clientX - r.left}px`);
        el.style.setProperty("--my", `${e.clientY - r.top}px`);
      });
    });

    document.querySelectorAll(".magnetic").forEach((el) => {
      el.addEventListener("pointermove", (e) => {
        const r = el.getBoundingClientRect();
        const x = (e.clientX - r.left - r.width / 2) * 0.25;
        const y = (e.clientY - r.top - r.height / 2) * 0.35;
        el.style.transform = `translate(${x}px, ${y}px)`;
      });
      el.addEventListener("pointerleave", () => (el.style.transform = ""));
    });
  }

  // ---------- Ribbon: a living voice made of light ----------
  const pointer = { x: -9999, y: -9999 };
  addEventListener("pointermove", (e) => { pointer.x = e.clientX; pointer.y = e.clientY; }, { passive: true });
  const voice = { level: 0.15, target: 0.15 };

  class Ribbon {
    constructor(canvas) {
      this.c = canvas;
      this.ctx = canvas.getContext("2d");
      this.calm = canvas.hasAttribute("data-calm");
      this.visible = false;
      this.t = Math.random() * 10;
      this.bulge = 0;
      new IntersectionObserver(([en]) => (this.visible = en.isIntersecting)).observe(canvas);
      this.resize();
      addEventListener("resize", () => this.resize());
    }
    resize() {
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const r = this.c.getBoundingClientRect();
      this.w = r.width; this.h = r.height; this.dpr = dpr;
      this.c.width = Math.round(r.width * dpr);
      this.c.height = Math.round(r.height * dpr);
      this.strands = r.width < 700 ? 14 : 24;
    }
    draw(dt) {
      if (!this.visible) return;
      const { ctx, w, h, dpr } = this;
      this.t += dt;
      const t = this.t;
      const level = this.calm ? 0.32 + 0.06 * Math.sin(t * 0.7) : voice.level;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      ctx.globalCompositeOperation = "lighter";

      const r = this.c.getBoundingClientRect();
      const px = pointer.x - r.left, py = pointer.y - r.top;
      // The hero ribbon flows straight through the microphone button.
      let holdY = null;
      if (!this.calm && holdBtn) {
        const b = holdBtn.getBoundingClientRect();
        holdY = b.top + b.height / 2 - r.top;
      }
      const near = px > 0 && px < w && py > 0 && py < h ? 1 : 0;
      this.bulge += (near - this.bulge) * 0.05;

      const cy = holdY ?? h * (this.calm ? 0.5 : 0.7);
      const amp = h * (this.calm ? 0.12 : 0.14) * (0.35 + level * 1.15);
      const grad = ctx.createLinearGradient(0, 0, w, 0);
      grad.addColorStop(0, "rgba(232,87,58,0)");
      grad.addColorStop(0.25, "rgba(232,87,58,1)");
      grad.addColorStop(0.55, "rgba(255,170,120,1)");
      grad.addColorStop(0.8, "rgba(232,87,58,1)");
      grad.addColorStop(1, "rgba(232,87,58,0)");
      ctx.strokeStyle = grad;

      const n = this.strands;
      const step = w < 700 ? 8 : 6;
      for (let s = 0; s < n; s++) {
        const k = s / (n - 1);
        const center = 1 - Math.abs(k - 0.5) * 2;
        ctx.globalAlpha = 0.05 + 0.32 * center * center;
        ctx.lineWidth = 0.6 + 1.1 * center;
        ctx.beginPath();
        for (let x = 0; x <= w + step; x += step) {
          const nx = x / w;
          const env = Math.pow(Math.sin(Math.PI * clamp(nx)), 1.5);
          let y =
            Math.sin(nx * 6.2 + t * 0.9 + k * 1.8) * 0.55 +
            Math.sin(nx * 11.5 - t * 1.4 + k * 3.4) * 0.3 * (0.4 + level) +
            Math.sin(nx * 23 + t * 3.1 + k * 6) * 0.15 * level;
          y *= amp * env * (0.45 + 0.55 * Math.sin(k * Math.PI));
          const dx = x - px;
          y += this.bulge * Math.exp(-(dx * dx) / (2 * 150 * 150)) * (py - cy) * 0.35 * Math.sin(k * Math.PI);
          const yy = cy + y + (k - 0.5) * 18 * (1 - level * 0.5);
          if (x === 0) ctx.moveTo(x, yy); else ctx.lineTo(x, yy);
        }
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
      ctx.globalCompositeOperation = "source-over";
    }
  }

  const ribbons = reduceMotion ? [] : [...document.querySelectorAll("[data-ribbon]")].map((c) => new Ribbon(c));
  let last = performance.now();
  function frame(now) {
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    const s = now / 1000;
    const speech = Math.abs(Math.sin(s * 7.3) * Math.sin(s * 2.3 + 1)) * 0.6 + Math.abs(Math.sin(s * 13.1)) * 0.4;
    const target = holding ? 0.45 + 0.55 * speech : 0.15 + 0.05 * Math.sin(s * 0.8);
    voice.level += (target - voice.level) * (holding ? 0.18 : 0.05);
    ribbons.forEach((r) => r.draw(dt));
    requestAnimationFrame(frame);
  }

  // ---------- Hold to speak (hero) ----------
  const holdBtn = document.querySelector("[data-hold]");
  let holdLabel = document.querySelector("[data-hold-label]");
  const typed = document.querySelector("[data-typed]");
  const typedText = document.querySelector("[data-typed-text]");
  const hero = document.querySelector(".hero");
  const phrases = [
    "Running ten minutes late. Start without me and I'll catch up.",
    "Thank you for staying late on Thursday. The launch went smoothly because of you.",
    "Remind me to call Grandma about the trip in December.",
    "Can we move the design review to Friday so the whole team can make it?",
    "Love this. Let's ship it on Monday.",
  ];
  let phraseIndex = 0;
  let holding = false;
  let holdStart = 0;
  let userTried = false;
  let heroVisible = true;
  new IntersectionObserver(([en]) => (heroVisible = en.isIntersecting), { threshold: 0.3 }).observe(hero);

  const startHold = () => {
    if (holding) return;
    holding = true;
    holdStart = performance.now();
    holdBtn.classList.add("is-on");
    holdLabel.textContent = "Listening… release";
    typedText.textContent = "";
    typed.classList.remove("has-text");
  };
  const endHold = () => {
    if (!holding) return;
    holding = false;
    holdBtn.classList.remove("is-on");
    holdLabel.textContent = finePointer ? "Hold" : "Press and hold";
    const phrase = phrases[phraseIndex++ % phrases.length];
    typed.classList.add("has-text");
    typedText.innerHTML = phrase.split(" ").map((w, i) => `<span class="tw" style="animation-delay:${i * 70}ms">${w}</span>`).join("");
  };
  if (holdBtn) {
    if (!finePointer) {
      document.getElementById("hold-hint").innerHTML = '<span data-hold-label>Press and hold</span> the button to try it';
      holdLabel = document.querySelector("[data-hold-label]");
    }
    holdBtn.addEventListener("pointerdown", (e) => { e.preventDefault(); userTried = true; startHold(); });
    addEventListener("pointerup", endHold);
    addEventListener("pointercancel", endHold);
    holdBtn.addEventListener("keydown", (e) => { if (e.key === "Enter") { userTried = true; startHold(); } });
    holdBtn.addEventListener("keyup", (e) => e.key === "Enter" && endHold());
    addEventListener("keydown", (e) => {
      if (e.code !== "Space" || e.repeat || !heroVisible) return;
      if (e.target.closest("input, textarea, button, a, summary, [contenteditable]")) return;
      e.preventDefault();
      userTried = true;
      startHold();
    });
    addEventListener("keyup", (e) => e.code === "Space" && endHold());

    // Until someone tries it, the page demonstrates itself now and then.
    (async () => {
      await sleep(2600);
      while (!userTried && !reduceMotion) {
        if (heroVisible && !holding) {
          startHold();
          holdLabel.textContent = "Listening…";
          await sleep(1800);
          if (!userTried) endHold();
        }
        await sleep(5200);
      }
    })();
  }

  // ---------- Pinned story: messy speech to clean text ----------
  const story = document.querySelector("[data-story]");
  const storyText = document.querySelector("[data-story-text]");
  const tokens = [
    ["um ", "f"], ["so ", "f"], ["I "], ["think "], ["we "], ["should "], ["uh ", "f"], ["move "], ["the "], ["design "],
    ["review "], ["to "], ["Thursday ", "f"], ["no ", "f"], ["wait ", "f"], ["Friday "], ["so "], ["the "], ["whole "],
    ["team "], ["can "], ["make "], ["it"], [".", "a"],
  ];
  if (story && storyText) {
    storyText.innerHTML = tokens
      .map(([w, k]) => `<span class="sw${k === "f" ? " filler" : ""}${k === "a" ? " add" : ""}">${w}</span>`)
      .join("");
    const words = [...storyText.children];
    const spoken = words.filter((w) => !w.classList.contains("add"));
    const added = words.filter((w) => w.classList.contains("add"));
    const fillers = words.filter((w) => w.classList.contains("filler"));
    const steps = [...document.querySelectorAll("[data-step]")];
    const bar = document.querySelector("[data-story-bar]");
    const fbar = document.querySelector("[data-story-bar-ui]");
    const label = document.querySelector("[data-story-label]");
    const time = document.querySelector("[data-story-time]");
    const toast = document.querySelector("[data-story-toast]");
    const wave = document.querySelector("[data-story-wave]");
    wave.innerHTML = "<i></i>".repeat(16);
    const bars = [...wave.children];
    let mode = "idle";

    const pinned = () => matchMedia("(min-width: 981px)").matches && !reduceMotion;
    let ticking = false;
    const update = () => {
      ticking = false;
      let p;
      if (pinned()) {
        const r = story.getBoundingClientRect();
        p = clamp(-r.top / (r.height - innerHeight));
      } else {
        // Unpinned: play through as the window scrolls past.
        const r = document.querySelector(".story-stage").getBoundingClientRect();
        p = reduceMotion ? 1 : clamp(map(innerHeight - r.top, 0, innerHeight * 1.1));
      }
      bar.style.transform = `scaleX(${p})`;
      const rec = p > 0.06 && p < 0.38;
      const busy = p >= 0.38 && p < 0.58;
      const polish = p >= 0.58 && p < 0.8;
      const done = p >= 0.8;
      const step = done ? 3 : polish ? 2 : busy ? 1 : 0;
      steps.forEach((s, i) => s.classList.toggle("on", i === step));

      mode = rec ? "rec" : busy || polish ? "busy" : "idle";
      fbar.classList.toggle("rec", rec);
      label.textContent = rec ? "Listening" : busy ? "Transcribing locally" : polish ? "Polishing" : done ? "Inserted in Slack" : "Hold fn";
      const secs = Math.round(map(p, 0.06, 0.38) * 7);
      time.textContent = `0:0${secs}`;

      const shown = Math.round(map(p, 0.38, 0.56) * spoken.length);
      spoken.forEach((w, i) => w.classList.toggle("in", i < shown));
      fillers.forEach((w) => {
        w.classList.toggle("marked", p >= 0.6);
        w.classList.toggle("gone", p >= 0.7);
      });
      added.forEach((w) => w.classList.toggle("in", p >= 0.72));
      toast.classList.toggle("on", done);
    };
    const onScroll = () => { if (!ticking) { ticking = true; requestAnimationFrame(update); } };
    addEventListener("scroll", onScroll, { passive: true });
    addEventListener("resize", onScroll);
    update();

    (function animateBar(now) {
      const t = now / 1000;
      bars.forEach((b, i) => {
        let v = 0.15;
        if (mode === "rec") v = 0.2 + 0.8 * Math.abs(Math.sin(t * 6 + i * 0.7) * Math.cos(t * 2.3 + i * 0.3));
        else if (mode === "busy") v = 0.18 + 0.3 * Math.max(0, Math.sin(t * 5 - i * 0.45));
        b.style.transform = `scaleY(${v.toFixed(3)})`;
      });
      requestAnimationFrame(animateBar);
    })(0);
  }

  // ---------- Statement: words light up as they cross a line ----------
  const statement = document.querySelector("[data-words]");
  if (statement) {
    const wrapWords = (node) => {
      [...node.childNodes].forEach((n) => {
        if (n.nodeType === 3) {
          const frag = document.createDocumentFragment();
          n.textContent.split(/(\s+)/).forEach((part) => {
            if (!part) return;
            if (/^\s+$/.test(part)) frag.append(" ");
            else { const s = document.createElement("span"); s.className = "w"; s.textContent = part; frag.append(s); }
          });
          n.replaceWith(frag);
        } else if (n.nodeType === 1) wrapWords(n);
      });
    };
    wrapWords(statement);
    const spans = [...statement.querySelectorAll(".w")];
    if (reduceMotion) spans.forEach((s) => s.classList.add("on"));
    else {
      let tick = false;
      const upd = () => {
        tick = false;
        const r = statement.getBoundingClientRect();
        const prog = (innerHeight * 0.78 - r.top) / (r.height + innerHeight * 0.3);
        const lit = Math.round(clamp(prog) * spans.length);
        spans.forEach((s, i) => s.classList.toggle("on", i < lit));
      };
      addEventListener("scroll", () => { if (!tick) { tick = true; requestAnimationFrame(upd); } }, { passive: true });
      upd();
    }
  }

  // ---------- Tour: tilt flattens as it scrolls in, tabs ----------
  const tilt = document.querySelector("[data-tilt]");
  if (tilt && !reduceMotion) {
    let tk = false;
    const upd = () => {
      tk = false;
      const r = tilt.getBoundingClientRect();
      const v = clamp((r.top - innerHeight * 0.15) / (innerHeight * 0.7));
      tilt.firstElementChild.style.setProperty("--tilt", v.toFixed(3));
    };
    addEventListener("scroll", () => { if (!tk) { tk = true; requestAnimationFrame(upd); } }, { passive: true });
    upd();
  }
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

  // ---------- Feature tiles that act out what they describe ----------
  const cap = document.querySelector("[data-cap]");
  const capLabel = document.querySelector("[data-cap-label]");
  if (cap && !reduceMotion) {
    const states = [["idle", ""], ["rec", "Listening"], ["busy", "Transcribing locally"], ["done", "✓ Inserted"]];
    let i = 0;
    const run = () => {
      const [s, l] = states[i++ % states.length];
      cap.dataset.state = s;
      capLabel.textContent = l;
    };
    run();
    setInterval(run, 1700);
  }
  const polishViz = document.querySelector("[data-polish]");
  if (polishViz && !reduceMotion) {
    (async () => {
      for (;;) {
        polishViz.className = "tile-visual polish-viz";
        await sleep(1600);
        polishViz.classList.add("mark");
        await sleep(1300);
        polishViz.classList.add("clean");
        await sleep(2600);
      }
    })();
  } else polishViz?.classList.add("mark", "clean");

  requestAnimationFrame(frame);
})();

// The hero's living arc-reactor core — a faithful vanilla port of the app's
// ArcCore component. Zero assets, zero network, pure canvas. This is the
// genuinely-premium first impression that needs nothing generated.
// Under prefers-reduced-motion it paints one static frame and stops.

const canvas = document.getElementById("hero-core");
if (canvas) {
  const ctx = canvas.getContext("2d");
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Palette pulled from the same tokens the app uses.
  const ACCENT = "#38c6f4";
  const ACCENT2 = "#56dfb4";
  const DIM = "#64788f";

  let raf = 0;

  function draw(t) {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const size = canvas.clientWidth;
    if (canvas.width !== size * dpr || canvas.height !== size * dpr) {
      canvas.width = size * dpr;
      canvas.height = size * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size, size);

    const c = size / 2;
    const R = size / 2 - 10;
    // Hero sits in a calm "idle-plus" state: alive, unhurried.
    const energy = 0.45;
    const spin = 0.00028;
    const breath = 0.5 + 0.5 * Math.sin(t * 0.0016);

    // Outer tick ring — 72 marks, a deterministic scatter lit brighter.
    for (let i = 0; i < 72; i++) {
      const a = (i / 72) * Math.PI * 2 + t * spin;
      const major = i % 6 === 0;
      const lit = (i * 7) % 13 < 4;
      const len = major ? 12 : 6;
      ctx.strokeStyle = lit ? ACCENT2 : ACCENT;
      ctx.globalAlpha = lit ? 0.5 + 0.4 * breath : major ? 0.5 : 0.24;
      ctx.lineWidth = major ? 1.6 : 1;
      ctx.beginPath();
      ctx.moveTo(c + Math.cos(a) * (R - len), c + Math.sin(a) * (R - len));
      ctx.lineTo(c + Math.cos(a) * R, c + Math.sin(a) * R);
      ctx.stroke();
    }

    // Counter-rotating arc segments + a hairline circle.
    const arc = (radius, start, sweep, width, alpha) => {
      ctx.strokeStyle = ACCENT;
      ctx.globalAlpha = alpha;
      ctx.lineWidth = width;
      ctx.beginPath();
      ctx.arc(c, c, radius, start, start + sweep);
      ctx.stroke();
    };
    arc(R - 22, t * spin * 2.4, Math.PI * 0.62, 2.4, 0.8);
    arc(R - 22, t * spin * 2.4 + Math.PI, Math.PI * 0.2, 2.4, 0.5);
    arc(R - 32, -t * spin * 1.6, Math.PI * 0.34, 1.2, 0.55);
    ctx.globalAlpha = 0.28;
    ctx.strokeStyle = ACCENT;
    ctx.lineWidth = 0.8;
    ctx.beginPath();
    ctx.arc(c, c, R - 42, 0, Math.PI * 2);
    ctx.stroke();

    // Radial voice bars — the core's breath.
    const BARS = 56;
    for (let i = 0; i < BARS; i++) {
      const a = (i / BARS) * Math.PI * 2 - Math.PI / 2;
      const phase = t * 0.0015 + i * 0.6;
      const wobble = 0.55 + 0.45 * (Math.sin(phase) * 0.6 + Math.sin(phase * 1.7 + 2) * 0.4);
      const amp = Math.max(0.06, energy * wobble);
      const r0 = R - 60;
      const r1 = r0 + amp * 16;
      ctx.strokeStyle = ACCENT;
      ctx.globalAlpha = 0.22 + amp * 0.6;
      ctx.lineWidth = 1.6;
      ctx.beginPath();
      ctx.moveTo(c + Math.cos(a) * r0, c + Math.sin(a) * r0);
      ctx.lineTo(c + Math.cos(a) * r1, c + Math.sin(a) * r1);
      ctx.stroke();
    }

    // Iris — the one glow.
    const irisR = R - 78;
    const glow = ctx.createRadialGradient(c, c, 2, c, c, irisR + 18);
    glow.addColorStop(0, ACCENT);
    glow.addColorStop(0.55, ACCENT);
    glow.addColorStop(1, "transparent");
    ctx.globalAlpha = 0.26 + 0.3 * breath;
    ctx.fillStyle = glow;
    ctx.beginPath();
    ctx.arc(c, c, irisR + 18, 0, Math.PI * 2);
    ctx.fill();

    ctx.globalAlpha = 0.9;
    ctx.strokeStyle = ACCENT;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(c, c, irisR, 0, Math.PI * 2);
    ctx.stroke();

    ctx.globalAlpha = 0.5 + 0.5 * breath;
    ctx.fillStyle = ACCENT;
    ctx.beginPath();
    ctx.arc(c, c, 4, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  if (reduceMotion) {
    // One calm static frame, no loop.
    requestAnimationFrame(() => draw(1400));
  } else {
    const loop = (t) => {
      draw(t);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    // Pause the loop when the hero scrolls out of view — no wasted frames.
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !raf) {
          raf = requestAnimationFrame(loop);
        } else if (!entry.isIntersecting && raf) {
          cancelAnimationFrame(raf);
          raf = 0;
        }
      },
      { threshold: 0.01 },
    );
    io.observe(canvas);
  }
}

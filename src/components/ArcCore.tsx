import { useEffect, useRef } from "react";
import type { Theme } from "../lib/theme";

export type CoreState = "offline" | "idle" | "thinking" | "speaking" | "listening";

/// Per-state motion profile: how alive the core is and how fast it spins.
export function stateProfile(state: CoreState): { energy: number; spin: number } {
  switch (state) {
    case "thinking":
      return { energy: 1, spin: 0.0011 };
    case "speaking":
      return { energy: 0.75, spin: 0.0006 };
    case "listening":
      return { energy: 0.55, spin: 0.0004 };
    case "idle":
      return { energy: 0.35, spin: 0.00022 };
    case "offline":
      return { energy: 0.06, spin: 0.00022 };
  }
}

interface Props {
  state: CoreState;
  theme: Theme; // re-read palette when the theme flips
  /// Self-rated confidence (0-100) of the latest answer; drawn as a gauge.
  confidence?: number | null;
}

interface Palette {
  accent: string;
  accent2: string;
  dim: string;
  line: string;
  warn: string;
}

// The arc-reactor core: the assistant's physical presence in the HUD.
// Concentric instrument rings around a glowing iris — dim and still when no
// model is available, breathing when idle, spinning hard while thinking.
// One canvas, ~220px, redrawn per frame; the only glow in the interface.
export default function ArcCore({ state, theme, confidence }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stateRef = useRef<CoreState>(state);
  stateRef.current = state;
  const confidenceRef = useRef<number | null>(confidence ?? null);
  confidenceRef.current = confidence ?? null;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const styles = getComputedStyle(document.documentElement);
    const palette: Palette = {
      accent: styles.getPropertyValue("--accent").trim() || "#38c6f4",
      accent2: styles.getPropertyValue("--accent-2").trim() || "#56dfb4",
      dim: styles.getPropertyValue("--text-dim").trim() || "#64788f",
      line: styles.getPropertyValue("--line-strong").trim() || "#3a4a5c",
      warn: styles.getPropertyValue("--warn").trim() || "#f0b23e",
    };
    const reduceMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;

    let raf = 0;

    const draw = (t: number) => {
      const dpr = window.devicePixelRatio || 1;
      const size = canvas.clientWidth;
      if (canvas.width !== size * dpr || canvas.height !== size * dpr) {
        canvas.width = size * dpr;
        canvas.height = size * dpr;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, size, size);

      const mode = stateRef.current;
      const c = size / 2;
      const R = size / 2 - 6;
      const live = mode !== "offline";
      const { energy, spin } = stateProfile(mode);
      const main = live ? palette.accent : palette.dim;
      const breath =
        0.5 + 0.5 * Math.sin(t * (mode === "thinking" || mode === "speaking" ? 0.006 : 0.0016));

      // Outer tick ring: 72 marks, a deterministic scatter of them lit.
      for (let i = 0; i < 72; i++) {
        const a = (i / 72) * Math.PI * 2 + t * spin;
        const major = i % 6 === 0;
        const lit = live && (i * 7) % 13 < 4;
        const len = major ? 9 : 5;
        ctx.strokeStyle = lit ? palette.accent2 : main;
        ctx.globalAlpha = lit ? 0.5 + 0.4 * breath : major ? 0.55 : 0.28;
        ctx.lineWidth = major ? 1.6 : 1;
        ctx.beginPath();
        ctx.moveTo(c + Math.cos(a) * (R - len), c + Math.sin(a) * (R - len));
        ctx.lineTo(c + Math.cos(a) * R, c + Math.sin(a) * R);
        ctx.stroke();
      }

      // Two counter-rotating arc segments + one hairline circle.
      const arc = (
        radius: number,
        start: number,
        sweep: number,
        width: number,
        alpha: number,
      ) => {
        ctx.strokeStyle = main;
        ctx.globalAlpha = alpha;
        ctx.lineWidth = width;
        ctx.beginPath();
        ctx.arc(c, c, radius, start, start + sweep);
        ctx.stroke();
      };
      arc(R - 16, t * spin * 2.4, Math.PI * 0.62, 2.4, live ? 0.8 : 0.35);
      arc(R - 16, t * spin * 2.4 + Math.PI, Math.PI * 0.2, 2.4, live ? 0.5 : 0.25);
      arc(R - 23, -t * spin * 1.6, Math.PI * 0.34, 1.2, live ? 0.55 : 0.25);
      ctx.globalAlpha = 0.3;
      ctx.strokeStyle = main;
      ctx.lineWidth = 0.8;
      ctx.beginPath();
      ctx.arc(c, c, R - 30, 0, Math.PI * 2);
      ctx.stroke();

      // Confidence gauge (§5.3): a 270° dial just inside the hairline ring.
      // Green-teal when sure, amber when it should have asked instead.
      const conf = confidenceRef.current;
      if (live && conf !== null) {
        const start = Math.PI * 0.75; // 135°, classic gauge origin
        const sweep = (Math.min(100, Math.max(0, conf)) / 100) * Math.PI * 1.5;
        ctx.strokeStyle = conf >= 70 ? palette.accent2 : conf >= 40 ? main : palette.warn;
        ctx.globalAlpha = 0.9;
        ctx.lineWidth = 2.6;
        ctx.beginPath();
        ctx.arc(c, c, R - 37, start, start + sweep);
        ctx.stroke();
        ctx.globalAlpha = 0.18;
        ctx.lineWidth = 2.6;
        ctx.beginPath();
        ctx.arc(c, c, R - 37, start + sweep, start + Math.PI * 1.5);
        ctx.stroke();
      }

      // Radial voice bars — the assistant's breath, between iris and rings.
      const BARS = 48;
      for (let i = 0; i < BARS; i++) {
        const a = (i / BARS) * Math.PI * 2 - Math.PI / 2;
        const phase =
          t * (mode === "thinking" || mode === "speaking" ? 0.005 : 0.0015) + i * 0.6;
        const wobble = 0.55 + 0.45 * (Math.sin(phase) * 0.6 + Math.sin(phase * 1.7 + 2) * 0.4);
        const amp = Math.max(0.06, energy * wobble);
        const r0 = R - 44;
        const r1 = r0 + amp * 12;
        ctx.strokeStyle = main;
        ctx.globalAlpha = 0.25 + amp * 0.6;
        ctx.lineWidth = 1.6;
        ctx.beginPath();
        ctx.moveTo(c + Math.cos(a) * r0, c + Math.sin(a) * r0);
        ctx.lineTo(c + Math.cos(a) * r1, c + Math.sin(a) * r1);
        ctx.stroke();
      }

      // Iris: the one glowing thing in the whole interface.
      const irisR = R - 58;
      const glow = ctx.createRadialGradient(c, c, 2, c, c, irisR + 14);
      const glowStrength = live ? 0.28 + 0.3 * breath * energy : 0.08;
      glow.addColorStop(0, main);
      glow.addColorStop(0.55, main);
      glow.addColorStop(1, "transparent");
      ctx.globalAlpha = glowStrength;
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(c, c, irisR + 14, 0, Math.PI * 2);
      ctx.fill();

      ctx.globalAlpha = live ? 0.9 : 0.4;
      ctx.strokeStyle = main;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(c, c, irisR, 0, Math.PI * 2);
      ctx.stroke();

      ctx.globalAlpha = live ? 0.5 + 0.5 * breath : 0.3;
      ctx.fillStyle = main;
      ctx.beginPath();
      ctx.arc(c, c, 3.2, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;
    };

    if (reduceMotion) {
      draw(400); // one static, non-zero frame
      return;
    }
    const loop = (t: number) => {
      draw(t);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [theme]);

  return (
    <div className="core" role="img" aria-label={`assistant core: ${state}`}>
      <canvas ref={canvasRef} className="core-canvas" />
    </div>
  );
}

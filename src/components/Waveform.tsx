import { useEffect, useRef } from "react";
import type { Theme } from "../lib/theme";

export type WaveState = "offline" | "idle" | "thinking";

interface Props {
  state: WaveState;
  theme: Theme; // re-read accent color when the theme flips
}

// Living activity visualizer: calm breathing bars when idle, energetic when
// thinking, near-flat when no model is available. GPU-cheap: one canvas,
// transform-free, and a single static frame under prefers-reduced-motion.
export default function Waveform({ state, theme }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stateRef = useRef<WaveState>(state);
  stateRef.current = state;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const styles = getComputedStyle(document.documentElement);
    const accent = styles.getPropertyValue("--accent").trim() || "#4fe3c1";
    const dim = styles.getPropertyValue("--text-dim").trim() || "#888";
    const reduceMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;

    const BARS = 56;
    let raf = 0;

    const draw = (t: number) => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
        canvas.width = w * dpr;
        canvas.height = h * dpr;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      const mode = stateRef.current;
      const energy = mode === "thinking" ? 1 : mode === "idle" ? 0.35 : 0.08;
      const speed = mode === "thinking" ? 0.006 : 0.0018;
      const step = w / BARS;

      for (let i = 0; i < BARS; i++) {
        const phase = t * speed + i * 0.55;
        const envelope = 0.4 + 0.6 * Math.sin((i / BARS) * Math.PI); // taller mid
        const wobble =
          Math.sin(phase) * 0.6 + Math.sin(phase * 1.7 + 2) * 0.4;
        const amp = Math.max(
          0.05,
          energy * envelope * (0.55 + 0.45 * wobble),
        );
        const barH = Math.max(2, amp * (h - 8));
        const x = i * step + step * 0.25;
        ctx.fillStyle = mode === "offline" ? dim : accent;
        ctx.globalAlpha = 0.35 + amp * 0.65;
        ctx.beginPath();
        ctx.roundRect(x, (h - barH) / 2, step * 0.5, barH, 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    };

    if (reduceMotion) {
      draw(1200); // one static, non-zero frame
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
    <div className="wave-panel" role="img" aria-label={`assistant activity: ${state}`}>
      <canvas ref={canvasRef} className="wave-canvas" />
    </div>
  );
}

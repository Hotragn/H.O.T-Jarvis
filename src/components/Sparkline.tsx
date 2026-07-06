import { useEffect, useRef } from "react";
import type { Theme } from "../lib/theme";

interface Props {
  values: number[]; // 0..100, oldest first
  theme: Theme;
  label: string;
}

// EKG-style trace for live telemetry. Data-driven, redrawn only when the
// sample array changes — no animation loop, so it costs nothing between polls.
export default function Sparkline({ values, theme, label }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const styles = getComputedStyle(document.documentElement);
    const accent = styles.getPropertyValue("--accent").trim() || "#38c6f4";
    const line = styles.getPropertyValue("--line").trim() || "#334455";

    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    // Baseline grid rule.
    ctx.strokeStyle = line;
    ctx.lineWidth = 1;
    ctx.globalAlpha = 0.8;
    ctx.beginPath();
    ctx.moveTo(0, h - 1);
    ctx.lineTo(w, h - 1);
    ctx.stroke();

    if (values.length < 2) return;
    const step = w / (values.length - 1);
    const y = (v: number) => h - 2 - (Math.min(100, Math.max(0, v)) / 100) * (h - 6);

    ctx.strokeStyle = accent;
    ctx.globalAlpha = 0.9;
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    values.forEach((v, i) => {
      const x = i * step;
      if (i === 0) ctx.moveTo(x, y(v));
      else ctx.lineTo(x, y(v));
    });
    ctx.stroke();

    // Bright head dot on the newest sample.
    const last = values[values.length - 1];
    ctx.fillStyle = accent;
    ctx.globalAlpha = 1;
    ctx.beginPath();
    ctx.arc(w - 1.5, y(last), 2, 0, Math.PI * 2);
    ctx.fill();
  }, [values, theme]);

  return (
    <canvas
      ref={canvasRef}
      className="sparkline"
      role="img"
      aria-label={label}
    />
  );
}

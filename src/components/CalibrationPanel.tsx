import { asPercent, asScore, biasPoints, binGap, binLabel, verdict } from "../lib/calibration";
import type { CalibrationReport } from "../lib/ipc";

interface Props {
  report: CalibrationReport | null;
}

// Confidence v1 (§5.3): the assistant's own track record. v0 showed a self-rating;
// this shows whether that rating was ever worth believing. A reliability diagram
// — stated confidence against how often it was actually right — plus the two
// standard scores and, most usefully, the signed bias in plain words.
export default function CalibrationPanel({ report }: Props) {
  if (!report) return null;
  const v = verdict(report);
  const points = biasPoints(report);

  return (
    <section className="calib" aria-label="confidence calibration">
      <div className="panel-title-row">
        <span className="panel-title">
          calibration · {report.sample_size} rated
        </span>
        <span className="calib-badge" data-verdict={v}>
          {v === "unknown" ? "not enough data" : v}
        </span>
      </div>

      <p className="panel-hint">{report.summary}</p>

      {report.trustworthy && (
        <>
          <div className="calib-stats">
            <div className="calib-stat">
              <span className="calib-stat-label">says</span>
              <span className="calib-stat-value">{asPercent(report.mean_confidence)}</span>
            </div>
            <div className="calib-stat">
              <span className="calib-stat-label">is right</span>
              <span className="calib-stat-value">{asPercent(report.accuracy)}</span>
            </div>
            <div className="calib-stat">
              <span className="calib-stat-label">gap</span>
              <span className="calib-stat-value" data-verdict={v}>
                {points > 0 ? `+${points}` : points}
              </span>
            </div>
            <div className="calib-stat" title="Brier score — lower is better, 0.25 is a coin flip">
              <span className="calib-stat-label">brier</span>
              <span className="calib-stat-value">{asScore(report.brier)}</span>
            </div>
            <div className="calib-stat" title="Expected calibration error — 0 is perfect">
              <span className="calib-stat-label">ece</span>
              <span className="calib-stat-value">{asScore(report.ece)}</span>
            </div>
          </div>

          {/* Reliability diagram: each band's claimed confidence (ghost bar)
              against what it actually delivered (solid bar). */}
          <ul className="calib-bins">
            {report.bins.map((bin) => {
              const gap = binGap(bin);
              return (
                <li className="calib-bin" key={bin.low}>
                  <span className="calib-bin-label">{binLabel(bin)}</span>
                  <span className="calib-bin-track">
                    <span
                      className="calib-bin-claimed"
                      style={{ width: `${Math.round(bin.mean_confidence * 100)}%` }}
                    />
                    <span
                      className="calib-bin-actual"
                      data-hot={gap > 0}
                      style={{ width: `${Math.round(bin.accuracy * 100)}%` }}
                    />
                  </span>
                  <span className="calib-bin-meta">
                    {asPercent(bin.accuracy)} · n={bin.count}
                  </span>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}

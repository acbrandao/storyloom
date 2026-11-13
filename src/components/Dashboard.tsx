// Library overview: headline numbers, storage composition, ratings, timeline, health.
import { useEffect, useState } from "react";
import { AlertTriangle, CalendarX, Clock, Copy, Film, FolderPlus, HardDrive, Image, Library, ListChecks, Music, RefreshCw, Star, Timer } from "lucide-react";
import { api, errorText } from "../api";
import { navigate, useStore } from "../store";
import type { BackgroundJob, LibraryPreset, LibraryStats, MediaType, Source } from "../types";
import { formatBytes, formatHours, formatMs } from "../format";
import { toast } from "./ui";

const SERIES: Record<MediaType, string> = { photo: "var(--series-1)", video: "var(--series-2)", audio: "var(--series-3)" };
const TYPE_LABEL: Record<MediaType, string> = { photo: "Photos", video: "Videos", audio: "Audio" };

// Hover tooltip shared by the charts.
function useTip() {
  const [tip, setTip] = useState<{ x: number; y: number; content: React.ReactNode } | null>(null);
  const bind = (content: React.ReactNode) => ({
    onMouseMove: (e: React.MouseEvent) => setTip({ x: e.clientX + 12, y: e.clientY + 12, content }),
    onMouseLeave: () => setTip(null),
  });
  const el = tip ? <div className="chart-tip" style={{ left: tip.x, top: tip.y }}>{tip.content}</div> : null;
  return { bind, el };
}

function StatTile({ icon, label, value, sub, onClick }: { icon: React.ReactNode; label: string; value: React.ReactNode; sub?: React.ReactNode; onClick?: () => void }) {
  return (
    <div className={`stat ${onClick ? "clickable" : ""}`} onClick={onClick}>
      <span className="stat-label">{icon}{label}</span>
      <span className="stat-value">{value}</span>
      {sub && <span className="stat-sub">{sub}</span>}
    </div>
  );
}

export function Dashboard({ stats, sources, onImport }: { stats: LibraryStats | null; sources: Source[]; onImport: () => void }) {
  const revision = useStore((s) => s.revision);
  const [jobs, setJobs] = useState<BackgroundJob[]>([]);
  const { bind, el } = useTip();
  useEffect(() => {
    api.recentJobs(8).then(setJobs).catch(() => {});
  }, [revision]);

  if (!stats) return <div className="page" />;
  if (stats.totalMedia === 0 && sources.length === 0) {
    return (
      <div className="empty-state">
        <Library size={56} />
        <h2>Welcome to StoryLoom</h2>
        <p>Add a folder of photos, videos or music to get started. StoryLoom only reads your files while indexing — nothing is moved, renamed or changed.</p>
        <button className="btn primary" onClick={onImport}><FolderPlus size={15} />Add a Folder…</button>
        <p className="subtle">Tip: you can also drag a folder from Finder onto this window.</p>
      </div>
    );
  }

  const lib = (preset: LibraryPreset) => () => navigate({ kind: "library", preset });
  const typeRows = (["photo", "video", "audio"] as MediaType[]).map((t) => stats.byType.find((b) => b.mediaType === t) ?? { mediaType: t, count: 0, bytes: 0 });
  const maxRating = Math.max(1, ...stats.ratingDistribution);
  const years = stats.byYear;
  const maxYear = Math.max(1, ...years.map((y) => y.photos + y.videos + y.audio));
  const maxCam = Math.max(1, ...stats.topCameras.map((c) => c.count));
  const indexing = stats.metadataPending + stats.thumbnailsPending;

  return (
    <div className="page">
      {el}
      <h2>Overview</h2>
      <div className="tiles">
        <StatTile icon={<Library size={14} />} label="Total media" value={stats.totalMedia.toLocaleString()} onClick={lib("all")} />
        <StatTile icon={<Image size={14} />} label="Photos" value={stats.totalPhotos.toLocaleString()} onClick={lib("photos")} />
        <StatTile icon={<Film size={14} />} label="Videos" value={stats.totalVideos.toLocaleString()} onClick={lib("videos")} />
        <StatTile icon={<Music size={14} />} label="Audio" value={stats.totalAudio.toLocaleString()} onClick={lib("audio")} />
        <StatTile icon={<HardDrive size={14} />} label="Storage" value={formatBytes(stats.totalBytes)} />
        <StatTile icon={<ListChecks size={14} />} label="Kept" value={stats.totalSelected.toLocaleString()} onClick={lib("picks")} />
        <StatTile icon={<Star size={14} />} label="Five-star" value={stats.totalFiveStar.toLocaleString()} onClick={() => navigate({ kind: "library", preset: "all" }, { minRating: 5 })} />
        <StatTile icon={<Timer size={14} />} label="Video duration" value={formatHours(stats.totalVideoSeconds)} />
      </div>
      {indexing > 0 && (
        <div className="card" style={{ marginTop: 12, display: "flex", alignItems: "center", gap: 12 }}>
          <RefreshCw size={15} className="spin" />
          <span>Indexing in progress — {stats.metadataPending.toLocaleString()} items waiting for details, {stats.thumbnailsPending.toLocaleString()} for thumbnails. You can keep working.</span>
        </div>
      )}

      <h2>Library</h2>
      <div className="cards">
        <div className="card">
          <h3>Storage by media type</h3>
          <div className="stack" style={{ marginBottom: 12 }}>
            {typeRows.filter((r) => r.bytes > 0).map((r) => (
              <div key={r.mediaType} style={{ width: `${(r.bytes / Math.max(1, stats.totalBytes)) * 100}%`, background: SERIES[r.mediaType] }}
                {...bind(<><b>{TYPE_LABEL[r.mediaType]}</b> · {formatBytes(r.bytes)} ({((r.bytes / Math.max(1, stats.totalBytes)) * 100).toFixed(1)}%)</>)} />
            ))}
          </div>
          <table className="data">
            <thead><tr><th>Media type</th><th className="num">Files</th><th className="num">Storage</th><th className="num">Share</th></tr></thead>
            <tbody>
              {typeRows.map((r) => (
                <tr key={r.mediaType} className="clickable" onClick={lib(r.mediaType === "photo" ? "photos" : r.mediaType === "video" ? "videos" : "audio")}>
                  <td><span className="swatch" style={{ background: SERIES[r.mediaType], marginRight: 8 }} />{TYPE_LABEL[r.mediaType]}</td>
                  <td className="num">{r.count.toLocaleString()}</td>
                  <td className="num">{formatBytes(r.bytes)}</td>
                  <td className="num">{stats.totalBytes ? ((r.bytes / stats.totalBytes) * 100).toFixed(1) : "0.0"}%</td>
                </tr>
              ))}
              <tr><td><b>Total</b></td><td className="num"><b>{stats.totalMedia.toLocaleString()}</b></td><td className="num"><b>{formatBytes(stats.totalBytes)}</b></td><td /></tr>
            </tbody>
          </table>
        </div>

        <div className="card">
          <h3>Ratings</h3>
          {[5, 4, 3, 2, 1, 0].map((r) => (
            <div className="hbar-row" key={r} {...bind(<><b>{r ? `${r} star${r > 1 ? "s" : ""}` : "Unrated"}</b> · {stats.ratingDistribution[r].toLocaleString()} items</>)}
              onClick={() => (r === 0 ? navigate({ kind: "library", preset: "unrated" }) : navigate({ kind: "library", preset: "all" }, { minRating: r, maxRating: r }))}>
              <span className="lbl">{r ? "★".repeat(r) : "Unrated"}</span>
              <div className="hbar-track"><div className="hbar-fill" style={{ width: `${(stats.ratingDistribution[r] / maxRating) * 100}%`, background: r === 0 ? "var(--border-strong)" : "var(--series-1)" }} /></div>
              <span className="val">{stats.ratingDistribution[r].toLocaleString()}</span>
            </div>
          ))}
        </div>

        <div className="card">
          <h3>Media by year</h3>
          <div className="legend">
            {(["photo", "video", "audio"] as MediaType[]).map((t) => <span key={t}><i className="swatch" style={{ background: SERIES[t] }} />{TYPE_LABEL[t]}</span>)}
          </div>
          {years.map((y) => {
            const total = y.photos + y.videos + y.audio;
            return (
              <div className="hbar-row" key={y.year} {...bind(<><b>{y.year}</b><br />Photos {y.photos.toLocaleString()} · Videos {y.videos.toLocaleString()} · Audio {y.audio.toLocaleString()}</>)}>
                <span className="lbl">{y.year}</span>
                <div className="hbar-track">
                  <div className="stack" style={{ width: `${(total / maxYear) * 100}%`, height: 12 }}>
                    {y.photos > 0 && <div style={{ flex: y.photos, background: SERIES.photo }} />}
                    {y.videos > 0 && <div style={{ flex: y.videos, background: SERIES.video }} />}
                    {y.audio > 0 && <div style={{ flex: y.audio, background: SERIES.audio }} />}
                  </div>
                </div>
                <span className="val">{total.toLocaleString()}</span>
              </div>
            );
          })}
          {!years.length && <span className="subtle">No dated media yet.</span>}
        </div>

        <div className="card">
          <h3>Cameras & devices</h3>
          {stats.topCameras.map((c) => (
            <div className="hbar-row" key={c.camera} {...bind(<><b>{c.camera}</b> · {c.count.toLocaleString()} items</>)}>
              <span className="lbl" title={c.camera}>{c.camera}</span>
              <div className="hbar-track"><div className="hbar-fill" style={{ width: `${(c.count / maxCam) * 100}%` }} /></div>
              <span className="val">{c.count.toLocaleString()}</span>
            </div>
          ))}
          {!stats.topCameras.length && <span className="subtle">No camera information found.</span>}
        </div>
      </div>

      <h2>Library health</h2>
      <div className="cards">
        <div className="card">
          <div className="health-list">
            <Health icon={<Star size={15} />} label="Unrated" value={stats.totalUnrated} onClick={lib("unrated")} />
            <Health icon={<Clock size={15} />} label="Marked for review" value={stats.totalNeedsReview} onClick={lib("review")} />
            <Health icon={<Copy size={15} />} label="Exact duplicate groups" value={stats.duplicateGroups} sub={stats.duplicateWastedBytes ? `${formatBytes(stats.duplicateWastedBytes)} in extra copies` : undefined} onClick={() => navigate({ kind: "duplicates" })} />
            <Health icon={<CalendarX size={15} />} label="No capture date" value={stats.withoutCaptureDate} onClick={lib("nodate")} />
            <Health icon={<AlertTriangle size={15} />} label="Missing or offline files" value={stats.missing} warn onClick={lib("missing")} />
            <Health icon={<AlertTriangle size={15} />} label="Files with unreadable details" value={stats.metadataErrors} warn />
          </div>
        </div>
        <div className="card">
          <h3>Source folders <button className="btn small" onClick={onImport}><FolderPlus size={13} />Add</button></h3>
          <table className="data">
            <thead><tr><th>Folder</th><th className="num">Files</th><th className="num">Storage</th><th>Last scan</th><th /></tr></thead>
            <tbody>
              {sources.map((s) => (
                <tr key={s.id} className="clickable" onClick={() => navigate({ kind: "folder", path: s.path, sourceId: s.id })}>
                  <td title={s.path}>{s.name}{s.isLibrary && <span className="status-pill" style={{ marginLeft: 6 }}>library</span>}{!s.isAvailable && <span className="status-pill failed" style={{ marginLeft: 6 }}>offline</span>}</td>
                  <td className="num">{s.mediaCount.toLocaleString()}</td>
                  <td className="num">{formatBytes(s.totalSize)}</td>
                  <td className="subtle">{s.lastScanAt ? formatMs(s.lastScanAt) : "—"}</td>
                  <td><button className="icon-btn" title="Rescan" onClick={async (e) => { e.stopPropagation(); try { await api.rescanSource(s.id); } catch (err) { toast(errorText(err), "error"); } }}><RefreshCw size={13} /></button></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="card">
          <h3>Recent imports & scans <button className="btn small ghost" onClick={() => navigate({ kind: "activity" })}>View all</button></h3>
          <table className="data">
            <tbody>
              {jobs.map((j) => (
                <tr key={j.id}>
                  <td className="subtle" style={{ whiteSpace: "nowrap" }}>{formatMs(j.createdAt)}</td>
                  <td>{j.jobType === "ingest" ? "Import" : j.jobType === "index" ? "Scan" : j.jobType}</td>
                  <td><span className={`status-pill ${j.status}`}>{j.status}</span></td>
                  <td className="num">{typeof j.summary?.new === "number" ? `+${(j.summary.new as number).toLocaleString()} new` : typeof j.summary?.copied === "number" ? `${j.summary.copied} copied` : ""}</td>
                </tr>
              ))}
              {!jobs.length && <tr><td className="subtle">No scans yet.</td></tr>}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function Health({ icon, label, value, sub, warn, onClick }: { icon: React.ReactNode; label: string; value: number; sub?: string; warn?: boolean; onClick?: () => void }) {
  return (
    <div className={`health-item ${onClick && value ? "clickable" : ""}`} onClick={value ? onClick : undefined}>
      <span style={{ color: warn && value ? "var(--review)" : "var(--text-3)" }}>{icon}</span>
      <span className="grow">{label}{sub && <div className="subtle">{sub}</div>}</span>
      <span className="num" style={{ color: warn && value ? "var(--review)" : undefined }}>{value.toLocaleString()}</span>
    </div>
  );
}

// Clip editor: mark in/out ranges on a video, keep them as draft clips, and export them.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, CheckCircle2, FolderSearch, Loader2, Pencil, Play, Plus, Scissors, Trash2, X } from "lucide-react";
import { api, errorText, filmstripUrl } from "../api";
import { bump } from "../store";
import type { ExportPlan, ExportPreset, FileDatePolicy, MediaDetail, Segment, ConflictPolicy } from "../types";
import { confirmDialog, Modal, toast } from "./ui";
import { waitForJob } from "./FileOpsDialogs";
import { formatLocal } from "../format";

// ---------------------------------------------------------------- time helpers

export function tc(sec: number): string {
  const ms = Math.round(Math.max(0, sec) * 1000);
  const h = Math.floor(ms / 3_600_000), m = Math.floor(ms / 60_000) % 60, s = Math.floor(ms / 1000) % 60, f = ms % 1000;
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(h)}:${p(m)}:${p(s)}.${p(f, 3)}`;
}

/** Parse "1:02:03.5", "2:03", "75.25" → seconds (null if invalid). */
export function parseTc(text: string): number | null {
  const parts = text.trim().split(":");
  if (!parts.length || parts.length > 3) return null;
  let total = 0;
  for (const p of parts) {
    if (!/^\d+(\.\d+)?$/.test(p)) return null;
    total = total * 60 + Number(p);
  }
  return total;
}

/** Source capture local time + seconds, formatted like the catalog (never re-zoned). */
export function addSeconds(local: string | null, sec: number): string | null {
  if (!local) return null;
  const m = local.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?/);
  if (!m) return null;
  const base = Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6], Math.round(Number(m[7] ?? 0) * 1000));
  const d = new Date(base + Math.round(sec * 1000));
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  const ms = d.getUTCMilliseconds();
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())}T${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}${ms ? "." + p(ms, 3) : ""}`;
}

function TimecodeInput({ value, onChange, label }: { value: number; onChange: (v: number) => void; label: string }) {
  const [text, setText] = useState(tc(value));
  const [focused, setFocused] = useState(false);
  useEffect(() => { if (!focused) setText(tc(value)); }, [value, focused]);
  const commit = () => {
    const v = parseTc(text);
    if (v == null) setText(tc(value));
    else onChange(v);
  };
  return (
    <input className="input timecode-input" aria-label={label} title={label} value={text} spellCheck={false}
      onFocus={() => setFocused(true)} onBlur={() => { setFocused(false); commit(); }}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setText(tc(value)); (e.target as HTMLInputElement).blur(); } }} />
  );
}

// ---------------------------------------------------------------- editor

export function ClipEditor({ d, video, time }: { d: MediaDetail; video: HTMLVideoElement | null; time: number }) {
  const duration = d.duration ?? video?.duration ?? 0;
  const fps = d.frameRate || 30;
  const [segments, setSegments] = useState<Segment[]>([]);
  const [range, setRange] = useState<[number, number]>(() => [time, Math.min(duration, time + 5)]);
  const [editing, setEditing] = useState<Segment | null>(null);
  const [name, setName] = useState("");
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [exporting, setExporting] = useState<number[] | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const stopAt = useRef<number | null>(null);
  const [strip, setStrip] = useState(true);

  const reload = useCallback(async () => {
    const segs = await api.segments(d.id);
    setSegments(segs);
    setChecked((c) => new Set([...c].filter((id) => segs.some((s) => s.id === id))));
  }, [d.id]);
  useEffect(() => { reload().catch(() => {}); }, [reload]);

  const snap = (t: number) => Math.max(0, Math.min(duration, Math.round(t * fps) / fps));
  const seek = (t: number) => { if (video) video.currentTime = t; };
  const setIn = (t: number) => setRange(([, o]) => [snap(Math.min(t, o - 1 / fps)), o]);
  const setOut = (t: number) => setRange(([i]) => [i, snap(Math.max(t, i + 1 / fps))]);

  // Stop "play selection" at the out-point.
  useEffect(() => {
    if (stopAt.current != null && time >= stopAt.current && video && !video.paused) {
      video.pause();
      stopAt.current = null;
    }
  }, [time, video]);

  // I / O keys set the in / out point at the playhead.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || e.metaKey || e.ctrlKey) return;
      if (e.key.toLowerCase() === "i") { setIn(video?.currentTime ?? 0); e.preventDefault(); }
      if (e.key.toLowerCase() === "o") { setOut(video?.currentTime ?? 0); e.preventDefault(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const timeAt = (clientX: number) => {
    const r = trackRef.current!.getBoundingClientRect();
    return snap(((clientX - r.left) / r.width) * duration);
  };
  const drag = (which: "in" | "out") => (e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    video?.pause();
    const move = (ev: PointerEvent) => {
      const t = timeAt(ev.clientX);
      if (which === "in") setIn(t); else setOut(t);
      seek(t); // scrub: show the exact frame under the handle
    };
    const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const save = async () => {
    try {
      if (editing) {
        await api.updateSegment(editing.id, range[0], range[1], name || null);
        toast("Clip updated", "success");
      } else {
        const id = await api.createSegment(d.id, range[0], range[1], name || null);
        setChecked((c) => new Set(c).add(id));
      }
      setEditing(null);
      setName("");
      await reload();
      bump();
    } catch (e) { toast(errorText(e), "error"); }
  };

  const edit = (s: Segment) => {
    setEditing(s);
    setName(s.name ?? "");
    setRange([s.inPoint, s.outPoint]);
    seek(s.inPoint);
  };

  const pct = (t: number) => `${(t / Math.max(duration, 0.001)) * 100}%`;
  const len = range[1] - range[0];
  const drafts = segments.filter((s) => !s.exportedPath);
  const selectedForExport = segments.filter((s) => checked.has(s.id));

  return (
    <div className="clip-editor">
      <div className="clip-track" ref={trackRef} onPointerDown={(e) => { if (e.target === e.currentTarget || (e.target as HTMLElement).classList.contains("clip-strip")) seek(timeAt(e.clientX)); }}>
        {strip && <img className="clip-strip" src={filmstripUrl(d.id)} onError={() => setStrip(false)} draggable={false} alt="" />}
        {segments.map((s, i) => (
          <div key={s.id} className={`clip-block ${s.exportedPath ? "exported" : "draft"} ${editing?.id === s.id ? "editing" : ""}`}
            style={{ left: pct(s.inPoint), width: pct(s.outPoint - s.inPoint) }} title={`${s.name ?? `Clip ${i + 1}`} · ${tc(s.inPoint)} – ${tc(s.outPoint)}`}
            onPointerDown={(e) => { e.stopPropagation(); if (!s.exportedPath) edit(s); else seek(s.inPoint); }} />
        ))}
        <div className="clip-range" style={{ left: pct(range[0]), width: pct(len) }}>
          <div className="clip-handle in" onPointerDown={drag("in")} title="Drag to set the in-point (I)" />
          <div className="clip-handle out" onPointerDown={drag("out")} title="Drag to set the out-point (O)" />
        </div>
        <div className="clip-playhead" style={{ left: pct(time) }} />
      </div>

      <div className="clip-controls">
        <button className="btn small" onClick={() => setIn(video?.currentTime ?? 0)} title="Set in-point at the playhead (I)">In</button>
        <TimecodeInput label="In-point" value={range[0]} onChange={(v) => { setIn(v); seek(v); }} />
        <span className="subtle">→</span>
        <TimecodeInput label="Out-point" value={range[1]} onChange={(v) => { setOut(v); seek(v); }} />
        <button className="btn small" onClick={() => setOut(video?.currentTime ?? 0)} title="Set out-point at the playhead (O)">Out</button>
        <span className="clip-stat">Length <b>{tc(len)}</b></span>
        <span className="clip-stat">Taken <b>{(() => { const c = addSeconds(d.userCaptureDatetime ?? d.captureDatetime, range[0]); return c ? formatLocal(c, true) : "unknown"; })()}</b></span>
        <button className="btn small" onClick={() => { if (video) { stopAt.current = range[1]; video.currentTime = range[0]; video.play(); } }}><Play size={12} />Play selection</button>
        <span className="spacer" />
        <input className="input" style={{ width: 170 }} placeholder="Clip name (optional)" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") save(); }} />
        {editing ? (
          <>
            <button className="btn small" onClick={() => { setEditing(null); setName(""); }}><X size={12} />Cancel</button>
            <button className="btn small primary" onClick={save}><CheckCircle2 size={12} />Save clip</button>
          </>
        ) : (
          <button className="btn small primary" onClick={save} disabled={len < 0.1}><Plus size={12} />Add clip</button>
        )}
      </div>

      <div className="clip-list">
        {segments.length === 0 ? (
          <div className="subtle" style={{ padding: "10px 4px" }}>Mark a range with the handles or the I / O keys, then <b>Add clip</b>. You can add as many clips from this video as you like.</div>
        ) : (
          <table className="data">
            <thead><tr><th style={{ width: 24 }}><input type="checkbox" checked={checked.size > 0 && checked.size === segments.length} onChange={(e) => setChecked(e.target.checked ? new Set(segments.map((s) => s.id)) : new Set())} /></th><th>Clip</th><th>In</th><th>Out</th><th>Length</th><th>Taken</th><th>Status</th><th /></tr></thead>
            <tbody>
              {segments.map((s, i) => {
                const cap = s.calculatedCaptureDatetime ?? addSeconds(d.userCaptureDatetime ?? d.captureDatetime, s.inPoint);
                return (
                  <tr key={s.id} className={editing?.id === s.id ? "editing-row" : ""}>
                    <td><input type="checkbox" checked={checked.has(s.id)} onChange={(e) => setChecked((c) => { const n = new Set(c); e.target.checked ? n.add(s.id) : n.delete(s.id); return n; })} /></td>
                    <td><a className="link" onClick={() => { seek(s.inPoint); }}>{s.name ?? `Clip ${i + 1}`}</a></td>
                    <td className="mono">{tc(s.inPoint)}</td>
                    <td className="mono">{tc(s.outPoint)}</td>
                    <td className="mono">{tc(s.outPoint - s.inPoint)}</td>
                    <td>{cap ? formatLocal(cap, true) : <span className="subtle">unknown</span>}</td>
                    <td>{s.exportedPath ? (
                      <span style={{ color: "var(--pick)" }} title={s.exportedPath}><CheckCircle2 size={11} /> Exported · {s.processingMethod === "stream_copy" ? "lossless" : "re-encoded"}</span>
                    ) : <span className="status-pill">Draft</span>}</td>
                    <td style={{ whiteSpace: "nowrap" }}>
                      <button className="icon-btn" title="Play" onClick={() => { if (video) { stopAt.current = s.outPoint; video.currentTime = s.inPoint; video.play(); } }}><Play size={13} /></button>
                      {s.exportedPath ? (
                        <button className="icon-btn" title="Show in Finder" onClick={() => api.revealPath(s.exportedPath!)}><FolderSearch size={13} /></button>
                      ) : (
                        <>
                          <button className="icon-btn" title="Edit range" onClick={() => edit(s)}><Pencil size={13} /></button>
                          <button className="icon-btn" title="Delete draft" onClick={async () => { await api.deleteSegment(s.id); if (editing?.id === s.id) setEditing(null); reload(); }}><Trash2 size={13} /></button>
                        </>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      <div className="clip-footer">
        <span className="subtle">{segments.length} clip{segments.length === 1 ? "" : "s"} · {drafts.length} not exported yet · Keys: I in, O out, Space play, , . frame step</span>
        <span className="spacer" />
        <button className="btn primary small" disabled={!selectedForExport.length && !drafts.length}
          onClick={() => setExporting((selectedForExport.length ? selectedForExport : drafts).map((s) => s.id))}>
          <Scissors size={13} />Export {selectedForExport.length || drafts.length} clip{(selectedForExport.length || drafts.length) === 1 ? "" : "s"}…
        </button>
      </div>
      {exporting && <ExportDialog ids={exporting} onClose={() => setExporting(null)} onDone={() => { reload(); bump(); }} />}
    </div>
  );
}

// ---------------------------------------------------------------- export dialog

const PRESETS: { id: ExportPreset; title: string; text: string }[] = [
  { id: "lossless", title: "Original quality (fast)", text: "No re-encoding, no quality loss. The clip may start slightly earlier, at the nearest keyframe." },
  { id: "exact", title: "Exact cut", text: "Frame-accurate. Re-encoded in the original format (H.264 or HEVC, 10-bit kept) at visually lossless quality." },
  { id: "compatible", title: "Exact, most compatible", text: "Frame-accurate H.264 MP4 that plays everywhere. HDR/10-bit sources are converted to 8-bit." },
];

export function ExportDialog({ ids, onClose, onDone }: { ids: number[]; onClose: () => void; onDone: () => void }) {
  const [destination, setDestination] = useState(() => localStorage.getItem("mc-clip-dest") ?? "");
  const [preset, setPreset] = useState<ExportPreset>(() => (localStorage.getItem("mc-clip-preset") as ExportPreset) ?? "lossless");
  const [template, setTemplate] = useState(() => localStorage.getItem("mc-clip-template") ?? "{capture_date}_{capture_time}_{location}_{description}_{sequence}");
  const [fileDates, setFileDates] = useState<FileDatePolicy>("sourceFile");
  const [conflict, setConflict] = useState<ConflictPolicy>("addSuffix");
  const [plan, setPlan] = useState<ExportPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const request = useMemo(() => ({ segmentIds: ids, destination, preset, nameTemplate: template, fileDates, conflict }), [ids, destination, preset, template, fileDates, conflict]);

  useEffect(() => {
    if (!destination) { setPlan(null); return; }
    setLoading(true);
    const t = setTimeout(async () => {
      try { setPlan(await api.planClipExport(request)); setError(null); } catch (e) { setPlan(null); setError(errorText(e)); }
      setLoading(false);
    }, 300);
    return () => clearTimeout(t);
  }, [request, destination]);

  const start = async () => {
    if (!plan) return;
    const ok = await confirmDialog({
      title: `Export ${plan.ready} clip${plan.ready === 1 ? "" : "s"}?`,
      message: <>New files are written to <b>{destination}</b>. The original videos are not changed. {preset === "lossless" ? "Clips keep the original quality." : "Clips are re-encoded for frame-accurate cuts."}</>,
      okLabel: "Export",
    });
    if (!ok) return;
    try {
      localStorage.setItem("mc-clip-dest", destination);
      localStorage.setItem("mc-clip-preset", preset);
      localStorage.setItem("mc-clip-template", template);
      const [job] = await api.startClipExport(request);
      onClose();
      toast(`Exporting ${plan.ready} clip${plan.ready === 1 ? "" : "s"}…`);
      const r = await waitForJob(job);
      onDone();
      let s: any = null;
      try { s = r.message ? JSON.parse(r.message) : null; } catch { /* ignore */ }
      if (r.status === "completed") {
        const failed = s?.failed?.length ?? 0;
        toast(`Exported ${s?.done ?? 0} clip(s)${failed ? `; ${failed} failed — see Activity` : ""}${s?.timestampWarnings?.length ? `; ${s.timestampWarnings.length} date warning(s)` : ""}`, failed ? "error" : "success", { label: "Show in Finder", onClick: () => api.revealPath(destination) });
      } else toast(`Export ${r.status}: ${r.message ?? ""}`, r.status === "cancelled" ? "info" : "error");
    } catch (e) { toast(errorText(e), "error"); }
  };

  return (
    <Modal title={`Export ${ids.length} clip${ids.length === 1 ? "" : "s"}`} wide onClose={onClose} footer={
      <>
        <span className="subtle" style={{ marginRight: "auto" }}>{loading ? <Loader2 size={13} className="spin" /> : plan && `${plan.ready} ready · ${tc(plan.totalSeconds)} total`}</span>
        <button className="btn" onClick={onClose}>Cancel</button>
        <button className="btn primary" disabled={!plan || plan.ready === 0 || loading} onClick={start}><Scissors size={13} />Export…</button>
      </>
    }>
      <div className="row" style={{ gap: 10, alignItems: "stretch" }}>
        {PRESETS.map((p) => (
          <label key={p.id} className={`choice ${preset === p.id ? "on" : ""}`} style={{ flex: 1 }}>
            <input type="radio" checked={preset === p.id} onChange={() => setPreset(p.id)} />
            <div><b>{p.title}</b><br /><span style={{ fontSize: 12 }}>{p.text}</span></div>
          </label>
        ))}
      </div>
      <div className="field">
        <span>Save clips to</span>
        <div className="path-row">
          <div className="path" title={destination}><bdi>{destination || "No folder chosen"}</bdi></div>
          <button className="btn" onClick={async () => { const p = await open({ directory: true, title: "Choose a folder for the clips" }); if (typeof p === "string") setDestination(p); }}>Choose…</button>
        </div>
      </div>
      <label className="field">
        <span>Clip names</span>
        <input className="input" value={template} onChange={(e) => setTemplate(e.target.value)} onKeyDown={(e) => e.stopPropagation()} spellCheck={false} style={{ fontFamily: "ui-monospace, SF Mono, Menlo, monospace" }} />
        <span className="hint">{"{capture_date} {capture_time}"} are the clip's own capture time (original + in-point). {"{description}"} uses the clip name if you gave one. Also: {"{location} {camera} {original_name} {sequence}"}.</span>
      </label>
      <details>
        <summary>Advanced</summary>
        <div className="row" style={{ gap: 14, marginTop: 10 }}>
          <label className="field"><span>File dates of the clips</span>
            <select className="select" value={fileDates} onChange={(e) => setFileDates(e.target.value as FileDatePolicy)}>
              <option value="sourceFile">Same as the original file (recommended)</option>
              <option value="clipCapture">The clip's capture time</option>
              <option value="exportTime">The time of export</option>
            </select>
          </label>
          <label className="field"><span>If a name is taken</span>
            <select className="select" value={conflict} onChange={(e) => setConflict(e.target.value as ConflictPolicy)}>
              <option value="addSuffix">Add a number (-2, -3…)</option>
              <option value="skip">Skip that clip</option>
            </select>
          </label>
        </div>
      </details>
      {error && <div className="error-text">{error}</div>}
      {plan && (
        <div className="card" style={{ maxHeight: 280, overflow: "auto", padding: 0 }}>
          <table className="data">
            <thead><tr><th>New file</th><th>From</th><th>Range</th><th>Taken</th><th>Notes</th></tr></thead>
            <tbody>
              {plan.items.map((i) => (
                <tr key={i.segmentId} style={i.status === "ready" ? undefined : { opacity: 0.6 }}>
                  <td style={{ fontSize: 11.5, wordBreak: "break-all" }}>{i.to.split("/").pop()}</td>
                  <td style={{ fontSize: 11.5 }}>{i.sourceFilename}</td>
                  <td className="mono" style={{ fontSize: 11.5, whiteSpace: "nowrap" }}>{tc(i.actualIn)} – {tc(i.actualOut)}</td>
                  <td style={{ fontSize: 11.5, whiteSpace: "nowrap" }}>{i.captureLocal ? <>{formatLocal(i.captureLocal, true)}{i.captureOffset && <span className="subtle"> {i.captureOffset}</span>}</> : <span className="subtle">unknown</span>}</td>
                  <td style={{ fontSize: 11.5 }}>
                    {i.status !== "ready" && <div className="error-text"><AlertTriangle size={11} /> {i.message}</div>}
                    {i.notes.map((n, k) => <div key={k} className={n.includes("earlier") || n.includes("8-bit") ? "warn" : "subtle"}>{n}</div>)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Modal>
  );
}

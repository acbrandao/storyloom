// Right panel: curation controls, tags, collections, notes and metadata for the selection.
import { useEffect, useMemo, useState } from "react";
import { Ban, Check, ChevronRight, Clock, FolderSearch, Heart, MapPin, Plus, X, CalendarClock, ExternalLink, AlertTriangle, PenLine, FolderInput, Scissors, Film } from "lucide-react";
import { api, errorText, thumbUrl } from "../api";
import { bump, bumpStructure, setState, useStore } from "../store";
import type { Collection, ColorLabel, MediaDetail, Tag } from "../types";
import { Modal, Stars, toast } from "./ui";
import { curate, addToCollection, openFileDialog } from "../actions";
import { CAPTURE_SOURCE_LABEL, formatBytes, formatDuration, formatLocal, formatMs } from "../format";

const COLORS: ColorLabel[] = ["red", "orange", "yellow", "green", "blue", "purple"];

export function Inspector({ tags, collections }: { tags: Tag[]; collections: Collection[] }) {
  const selection = useStore((s) => s.selection);
  const revision = useStore((s) => s.revision);
  const ids = useMemo(() => [...selection], [selection]);
  const [detail, setDetail] = useState<MediaDetail | null>(null);

  useEffect(() => {
    if (ids.length !== 1) { setDetail(null); return; }
    let live = true;
    api.mediaDetail(ids[0]).then((d) => live && setDetail(d)).catch(() => live && setDetail(null));
    return () => { live = false; };
  }, [ids, revision]);

  if (ids.length === 0) {
    return (
      <aside className="inspector">
        <div className="empty-state" style={{ height: "100%" }}>
          <p className="subtle">Select an item to see its details, rate it, add tags or notes.</p>
        </div>
      </aside>
    );
  }
  if (ids.length > 1) return <aside className="inspector"><BatchPanel ids={ids} tags={tags} collections={collections} /></aside>;
  if (!detail) return <aside className="inspector" />;
  return <aside className="inspector"><SinglePanel d={detail} tags={tags} collections={collections} /></aside>;
}

function CurationControls({ ids, rating, selection, review, favorite, color }: {
  ids: number[]; rating?: number; selection?: string; review?: string; favorite?: boolean; color?: ColorLabel | null;
}) {
  return (
    <>
      <div className="panel">
        <h3>Rating</h3>
        <Stars value={rating ?? 0} size={20} onChange={(r) => curate(ids, { rating: r })} />
      </div>
      <div className="panel">
        <h3>Status</h3>
        <div className="row">
          <button className={`flag-btn pick ${selection === "selected" ? "on" : ""}`} title="Keep (P)" onClick={() => curate(ids, { selectionStatus: selection === "selected" ? "none" : "selected" })}><Check size={13} />Keep</button>
          <button className={`flag-btn reject ${selection === "rejected" ? "on" : ""}`} title="Reject (X)" onClick={() => curate(ids, { selectionStatus: selection === "rejected" ? "none" : "rejected" })}><Ban size={13} />Reject</button>
          <button className={`flag-btn review ${review === "needs_review" ? "on" : ""}`} title="Needs review (R)" onClick={() => curate(ids, { reviewStatus: review === "needs_review" ? "none" : "needs_review" })}><Clock size={13} />Review</button>
          <button className={`flag-btn fav ${favorite ? "on" : ""}`} title="Favorite (F)" onClick={() => curate(ids, { favorite: !favorite })}><Heart size={13} />Favorite</button>
        </div>
        <div className="color-pick" style={{ marginTop: 10 }}>
          <button className={`none ${!color ? "on" : ""}`} title="No label" onClick={() => curate(ids, { colorLabel: "" })} />
          {COLORS.map((c) => <button key={c} className={`label-${c} ${color === c ? "on" : ""}`} title={c} onClick={() => curate(ids, { colorLabel: color === c ? "" : c })} />)}
        </div>
      </div>
    </>
  );
}

function TagEditor({ ids, current, tags }: { ids: number[]; current: Tag[]; tags: Tag[] }) {
  const [text, setText] = useState("");
  const [hl, setHl] = useState(0);
  const suggestions = useMemo(() => {
    const q = text.trim().toLowerCase();
    if (!q) return [];
    return tags.filter((t) => t.path.toLowerCase().includes(q) && !current.some((c) => c.id === t.id)).slice(0, 8);
  }, [text, tags, current]);
  const add = async (tag?: Tag) => {
    try {
      const id = tag ? tag.id : await api.createTag(text.trim().replace(/\s*\/\s*/g, "/"));
      await api.assignTag(ids, id);
      setText("");
      bumpStructure();
    } catch (e) { toast(errorText(e), "error"); }
  };
  return (
    <div className="panel">
      <h3>Tags</h3>
      <div className="row" style={{ marginBottom: 8 }}>
        {current.map((t) => (
          <span className="chip" key={t.id} title={t.path}>
            <span>{t.path}</span>
            <button onClick={async () => { await api.unassignTag(ids, t.id); bumpStructure(); }}><X size={11} /></button>
          </span>
        ))}
        {!current.length && <span className="subtle">No tags yet</span>}
      </div>
      <div className="autocomplete">
        <input className="input" style={{ width: "100%" }} placeholder={ids.length > 1 ? `Add tag to ${ids.length} items…` : "Add tag (e.g. Activities/Windsurfing)…"} value={text}
          onChange={(e) => { setText(e.target.value); setHl(0); }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "ArrowDown") { setHl((h) => Math.min(h + 1, suggestions.length - 1)); e.preventDefault(); }
            if (e.key === "ArrowUp") { setHl((h) => Math.max(h - 1, 0)); e.preventDefault(); }
            if (e.key === "Enter" && text.trim()) add(suggestions[hl]);
            if (e.key === "Escape") setText("");
          }} />
        {suggestions.length > 0 && (
          <div className="autocomplete-list">
            {suggestions.map((t, i) => <div key={t.id} className={i === hl ? "hl" : ""} onMouseDown={(e) => { e.preventDefault(); add(t); }}>{t.path}</div>)}
            <div className="subtle" onMouseDown={(e) => { e.preventDefault(); add(); }}>Create “{text.trim()}”</div>
          </div>
        )}
      </div>
    </div>
  );
}

function CollectionEditor({ ids, current, collections }: { ids: number[]; current: { id: number; name: string }[]; collections: Collection[] }) {
  const manual = collections.filter((c) => c.collectionType === "manual" && !current.some((x) => x.id === c.id));
  return (
    <div className="panel">
      <h3>Collections</h3>
      <div className="row">
        {current.map((c) => (
          <span className="chip" key={c.id}>
            <span>{c.name}</span>
            <button title="Remove from collection" onClick={async () => { await api.removeFromCollection(c.id, ids); bumpStructure(); }}><X size={11} /></button>
          </span>
        ))}
        {manual.length > 0 ? (
          <select className="select" value="" onChange={(e) => e.target.value && addToCollection(Number(e.target.value), ids)} style={{ height: 24, fontSize: 12 }}>
            <option value="">+ Add to collection…</option>
            {manual.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </select>
        ) : !current.length && <span className="subtle">Create a collection in the sidebar to group media.</span>}
      </div>
    </div>
  );
}

function TextField({ label, value, onSave, placeholder, multiline }: { label: string; value: string | null; onSave: (v: string) => void; placeholder?: string; multiline?: boolean }) {
  const [v, setV] = useState(value ?? "");
  useEffect(() => setV(value ?? ""), [value]);
  const commit = () => { if (v !== (value ?? "")) onSave(v); };
  const common = {
    className: "input", value: v, placeholder,
    onChange: (e: React.ChangeEvent<HTMLInputElement & HTMLTextAreaElement>) => setV(e.target.value),
    onBlur: commit,
    onKeyDown: (e: React.KeyboardEvent) => { e.stopPropagation(); if (e.key === "Enter" && !multiline) (e.target as HTMLElement).blur(); if (e.key === "Escape") { setV(value ?? ""); } },
    style: { width: "100%" },
  };
  return (
    <label className="field" style={{ marginBottom: 8 }}>
      <span>{label}</span>
      {multiline ? <textarea {...common} rows={3} /> : <input {...common} />}
    </label>
  );
}

function SinglePanel({ d, tags, collections }: { d: MediaDetail; tags: Tag[]; collections: Collection[] }) {
  const ids = [d.id];
  const [dateEdit, setDateEdit] = useState(false);
  const [raw, setRaw] = useState<Record<string, unknown> | null | undefined>(undefined);
  useEffect(() => setRaw(undefined), [d.id]);
  const effectiveDate = d.userCaptureDatetime ?? d.captureDatetime;
  const tz = d.userCaptureDatetime ? d.userCaptureTimezone : d.captureTimezone;
  const camera = [d.cameraMake && !d.cameraModel?.toLowerCase().startsWith(d.cameraMake.split(" ")[0].toLowerCase()) ? d.cameraMake : null, d.cameraModel].filter(Boolean).join(" ");
  return (
    <>
      <div className="inspector-preview" onDoubleClick={() => setState({ viewerId: d.id })}>
        <img src={thumbUrl(d.id, d.thumbnailStatus)} alt="" onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")} />
      </div>
      <div className="panel">
        <div className="filename">{d.currentFilename}</div>
        <div className="subtle" style={{ marginTop: 3 }}>
          {effectiveDate ? formatLocal(effectiveDate) : "Capture date unknown"}
          {d.width && d.height ? ` · ${d.width} × ${d.height}` : ""}
          {d.duration ? ` · ${formatDuration(d.duration)}` : ""}
          {` · ${formatBytes(d.fileSize)}`}
        </div>
        {d.isMissing && <div className="error-text" style={{ marginTop: 6, display: "flex", gap: 6, alignItems: "center" }}><AlertTriangle size={13} /> The file is missing or its drive is disconnected.</div>}
        {d.metadataStatus === "error" && <div className="warn" style={{ marginTop: 6, fontSize: 12 }}>Details could not be read: {d.metadataError}</div>}
        <div className="row" style={{ marginTop: 8 }}>
          <button className="btn small" onClick={() => api.revealInFinder(d.id)}><FolderSearch size={13} />Show in Finder</button>
          <button className="btn small" onClick={() => api.openExternal(d.id)}><ExternalLink size={13} />Open</button>
          <button className="btn small" onClick={() => openFileDialog("rename", [d.id])} title="Rename the file on disk (F2)"><PenLine size={13} />Rename</button>
        </div>
        {d.originalFilename !== d.currentFilename && <div className="subtle" style={{ marginTop: 6 }}>Originally “{d.originalFilename}”</div>}
      </div>
      <CurationControls ids={ids} rating={d.rating} selection={d.selectionStatus} review={d.reviewStatus} favorite={d.favorite} color={d.colorLabel} />
      {d.clipOrigin && (
        <div className="panel">
          <h3>Clip</h3>
          <div style={{ fontSize: 12.5 }}>
            From <a className="link" onClick={() => setState({ viewerId: d.clipOrigin!.sourceMediaId, clipMode: true })}><Film size={11} /> {d.clipOrigin.sourceFilename}</a>
            <div className="subtle mono" style={{ marginTop: 4 }}>{tcShort(d.clipOrigin.actualInPoint ?? d.clipOrigin.inPoint)} – {tcShort(d.clipOrigin.outPoint)} · {d.clipOrigin.processingMethod === "stream_copy" ? "lossless" : "re-encoded"}</div>
          </div>
        </div>
      )}
      {d.mediaType === "video" && !d.clipOrigin && (
        <div className="panel">
          <h3>Clips</h3>
          <div className="row">
            <span className="subtle">{d.derivedSegments ? `${d.derivedSegments} clip${d.derivedSegments === 1 ? "" : "s"} exported` : "No clips yet"}</span>
            <span className="spacer" />
            <button className="btn small" onClick={() => setState({ viewerId: d.id, clipMode: true })}><Scissors size={13} />Make clips</button>
          </div>
        </div>
      )}
      <TagEditor ids={ids} current={d.tags} tags={tags} />
      <CollectionEditor ids={ids} current={d.collections} collections={collections} />
      <div className="panel">
        <h3>Description & Notes</h3>
        <TextField label="Description" value={d.description} placeholder="What's happening here?" onSave={(v) => curate(ids, { description: v })} multiline />
        <TextField label="Location" value={d.location} placeholder="e.g. Sandy Hook, NJ" onSave={(v) => curate(ids, { location: v })} />
        <TextField label="Notes" value={d.notes} placeholder="Private notes" onSave={(v) => curate(ids, { notes: v })} multiline />
      </div>
      <div className="panel">
        <h3>
          Date & Time
          <button className="btn small ghost" onClick={() => setDateEdit(true)}><CalendarClock size={13} />Adjust…</button>
        </h3>
        <dl className="kv">
          <dt>Taken</dt>
          <dd>{effectiveDate ? <>{formatLocal(effectiveDate, true)}{tz ? <span className="subtle"> (UTC{tz})</span> : <span className="subtle"> (time zone not recorded)</span>}</> : <span className="warn">Unknown</span>}</dd>
          <dt>Source</dt>
          <dd>{d.userCaptureDatetime ? "Your correction" : d.captureTimeSource ? CAPTURE_SOURCE_LABEL[d.captureTimeSource] ?? d.captureTimeSource : "No date in file — sorted by file date"}</dd>
          {d.userCaptureDatetime && <><dt>Original</dt><dd>{d.captureDatetime ? formatLocal(d.captureDatetime, true) : "None recorded"}</dd></>}
          <dt>File created</dt><dd>{formatMs(d.filesystemCreationTime)}</dd>
          <dt>File modified</dt><dd>{formatMs(d.filesystemModificationTime)}</dd>
          <dt>Added</dt><dd>{formatMs(d.addedAt)}</dd>
        </dl>
      </div>
      <div className="panel">
        <h3>Information</h3>
        <dl className="kv">
          {camera && <><dt>Camera</dt><dd>{camera}</dd></>}
          {d.lens && <><dt>Lens</dt><dd>{d.lens}</dd></>}
          {d.exposure && <><dt>Exposure</dt><dd>{d.exposure}</dd></>}
          {d.gpsLatitude != null && d.gpsLongitude != null && <><dt>Location</dt><dd><MapPin size={11} /> {d.gpsLatitude.toFixed(5)}, {d.gpsLongitude.toFixed(5)}{d.gpsAltitude != null ? ` · ${Math.round(d.gpsAltitude)} m` : ""}</dd></>}
          {d.title && <><dt>Title</dt><dd>{d.title}</dd></>}
          {d.artist && <><dt>Artist</dt><dd>{d.artist}</dd></>}
          {d.album && <><dt>Album</dt><dd>{d.album}</dd></>}
          {d.genre && <><dt>Genre</dt><dd>{d.genre}</dd></>}
          {d.embeddedCaption && <><dt>Caption</dt><dd>{d.embeddedCaption}</dd></>}
          {d.embeddedKeywords && <><dt>Keywords</dt><dd>{d.embeddedKeywords}</dd></>}
          <dt>Folder</dt><dd style={{ wordBreak: "break-all" }}>{d.folderPath}</dd>
        </dl>
      </div>
      <div className="panel">
        <details>
          <summary><ChevronRight size={13} /> Advanced</summary>
          <dl className="kv" style={{ marginTop: 10 }}>
            <dt>Original name</dt><dd>{d.originalFilename}</dd>
            <dt>Original path</dt><dd style={{ wordBreak: "break-all" }}>{d.originalPath}</dd>
            <dt>Current path</dt><dd style={{ wordBreak: "break-all" }}>{d.currentPath}</dd>
            <dt>Type</dt><dd>{d.mimeType ?? d.extension}</dd>
            {d.codec && <><dt>Codec</dt><dd>{d.codec}{d.audioCodec && d.mediaType === "video" ? ` / ${d.audioCodec}` : ""}</dd></>}
            {d.frameRate && <><dt>Frame rate</dt><dd>{d.frameRate} fps</dd></>}
            {d.bitrate && <><dt>Bitrate</dt><dd>{(d.bitrate / 1_000_000).toFixed(2)} Mb/s</dd></>}
            {d.sampleRate && <><dt>Sample rate</dt><dd>{(d.sampleRate / 1000).toFixed(1)} kHz</dd></>}
            {d.orientation && <><dt>Orientation</dt><dd>{d.orientation}</dd></>}
            <dt>Capture field</dt><dd>{d.captureTimeSource ?? "—"}</dd>
            <dt>Content hash</dt><dd style={{ wordBreak: "break-all", fontSize: 11 }}>{d.fileHash ?? "Not computed (only same-size files are hashed)"}</dd>
            <dt>Media ID</dt><dd>{d.id}</dd>
          </dl>
          <div style={{ marginTop: 10 }}>
            {raw === undefined ? (
              <button className="btn small" onClick={() => api.metadataSnapshot(d.id).then(setRaw)}>Show original metadata</button>
            ) : raw ? (
              <div className="json">{JSON.stringify(raw, null, 2)}</div>
            ) : (
              <span className="subtle">No metadata snapshot.</span>
            )}
          </div>
        </details>
      </div>
      {dateEdit && <DateDialog d={d} onClose={() => setDateEdit(false)} />}
    </>
  );
}

function DateDialog({ d, onClose }: { d: MediaDetail; onClose: () => void }) {
  const initial = (d.userCaptureDatetime ?? d.captureDatetime ?? d.sortDatetime ?? "").slice(0, 19);
  const [local, setLocal] = useState(initial);
  const [offset, setOffset] = useState(d.userCaptureTimezone ?? d.captureTimezone ?? "");
  const save = async (clear = false) => {
    try {
      await api.setCaptureCorrection(d.id, clear ? null : local, clear ? null : offset || null);
      bump();
      onClose();
      toast(clear ? "Restored the original date" : "Date corrected (the file itself is unchanged)", "success");
    } catch (e) { toast(errorText(e), "error"); }
  };
  return (
    <Modal title="Adjust capture date" onClose={onClose} footer={
      <>
        {d.userCaptureDatetime && <button className="btn" onClick={() => save(true)}>Restore original</button>}
        <span className="spacer" />
        <button className="btn" onClick={onClose}>Cancel</button>
        <button className="btn primary" onClick={() => save()}>Save correction</button>
      </>
    }>
      <p style={{ margin: 0 }}>The correction is stored in StoryLoom’s catalog only. The original value from the file is kept and the file is not modified.</p>
      <div className="kv"><dt>From the file</dt><dd>{d.captureDatetime ? formatLocal(d.captureDatetime, true) : "No capture date recorded"}</dd></div>
      <label className="field"><span>Date & time (as shown on the camera)</span>
        <input className="input" type="datetime-local" step={1} value={local} onChange={(e) => setLocal(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
      </label>
      <label className="field"><span>Time zone offset (optional)</span>
        <input className="input" placeholder="e.g. -03:00" value={offset} onChange={(e) => setOffset(e.target.value)} onKeyDown={(e) => e.stopPropagation()} style={{ width: 120 }} />
      </label>
    </Modal>
  );
}

function BatchPanel({ ids, tags, collections }: { ids: number[]; tags: Tag[]; collections: Collection[] }) {
  return (
    <>
      <div className="panel">
        <div className="filename">{ids.length.toLocaleString()} items selected</div>
        <div className="subtle">Changes apply to every selected item.</div>
      </div>
      <CurationControls ids={ids} />
      <BatchTags ids={ids} tags={tags} />
      <CollectionEditor ids={ids} current={[]} collections={collections} />
      <div className="panel">
        <h3>Files</h3>
        <div className="row">
          <button className="btn small" onClick={() => openFileDialog("rename", ids)}><PenLine size={13} />Rename {ids.length} files…</button>
          <button className="btn small" onClick={() => openFileDialog("organize", ids)}><FolderInput size={13} />Copy or move to folder…</button>
        </div>
      </div>
      <div className="panel">
        <button className="btn small" onClick={async () => { await api.reprocess(ids); toast("Re-reading details and thumbnails…"); }}><Plus size={12} />Re-read file details</button>
      </div>
    </>
  );
}

/** Tags on any selected item, with counts; add to all or remove from all. */
function BatchTags({ ids, tags }: { ids: number[]; tags: Tag[] }) {
  const revision = useStore((s) => s.structureRevision);
  const [counts, setCounts] = useState<[number, number][]>([]);
  useEffect(() => {
    api.tagCounts(ids).then(setCounts).catch(() => setCounts([]));
  }, [ids, revision]);
  const current = counts.map(([id]) => tags.find((t) => t.id === id)).filter((t): t is Tag => !!t);
  return (
    <>
      <TagEditor ids={ids} current={[]} tags={tags} />
      {current.length > 0 && (
        <div className="panel" style={{ paddingTop: 0 }}>
          <div className="subtle" style={{ marginBottom: 6 }}>Tags on the selection (remove from all):</div>
          <div className="row">
            {current.map((t) => {
              const n = counts.find(([id]) => id === t.id)?.[1] ?? 0;
              return (
                <span className="chip" key={t.id} title={t.path}>
                  <span>{t.path} <span className="subtle">{n === ids.length ? "all" : `${n}/${ids.length}`}</span></span>
                  <button onClick={async () => { await api.unassignTag(ids, t.id); bumpStructure(); }}><X size={11} /></button>
                </span>
              );
            })}
          </div>
        </div>
      )}
    </>
  );
}

function tcShort(sec: number) {
  const ms = Math.round(sec * 1000);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(Math.floor(ms / 3_600_000))}:${p(Math.floor(ms / 60_000) % 60)}:${p(Math.floor(ms / 1000) % 60)}.${p(ms % 1000, 3)}`;
}

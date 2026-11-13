import { useEffect, useRef, useState } from "react";
import { ArrowDownWideNarrow, ArrowUpNarrowWide, FolderPlus, PanelLeft, PanelRight, Search, SlidersHorizontal, Sparkles, X, Info, FastForward } from "lucide-react";
import { api, errorText } from "../api";
import { activeFilterCount, bumpStructure, effectiveFilter, navigate, setState, stripEmpty, useStore } from "../store";
import type { ColorLabel, MediaFilter, MediaType, SortField, Tag } from "../types";
import { promptText, toast } from "./ui";

const SORTS: { value: SortField; label: string }[] = [
  { value: "captureDate", label: "Date taken" },
  { value: "addedAt", label: "Date added" },
  { value: "filename", label: "File name" },
  { value: "rating", label: "Rating" },
  { value: "fileSize", label: "File size" },
  { value: "duration", label: "Duration" },
];

export function Toolbar({ title, count, onImport, isGrid }: { title: string; count?: number; onImport: () => void; isGrid: boolean }) {
  const s = useStore((st) => st);
  const searchRef = useRef<HTMLInputElement>(null);
  const [text, setText] = useState(s.filter.text ?? "");
  useEffect(() => setText(s.filter.text ?? ""), [s.view]);
  // Debounce search typing.
  useEffect(() => {
    const t = setTimeout(() => {
      if ((s.filter.text ?? "") !== text) setState((st) => ({ filter: { ...st.filter, text: text || null } }));
    }, 180);
    return () => clearTimeout(t);
  }, [text]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const nFilters = activeFilterCount(s.filter);

  return (
    <div className={`toolbar drag ${s.leftOpen ? "" : "no-left"}`} data-tauri-drag-region>
      <button className={`icon-btn ${s.leftOpen ? "" : ""}`} title="Toggle sidebar" onClick={() => setState({ leftOpen: !s.leftOpen })}><PanelLeft size={16} /></button>
      <h1 data-tauri-drag-region>{title}</h1>
      {count != null && <span className="count" data-tauri-drag-region>{count.toLocaleString()} item{count === 1 ? "" : "s"}</span>}
      <span className="spacer" data-tauri-drag-region />
      {isGrid && (
        <>
          <div className="search">
            <Search size={14} />
            <input ref={searchRef} placeholder="Search names, tags, notes, camera…" value={text} onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Escape") { setText(""); (e.target as HTMLInputElement).blur(); } if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} />
            {text && <button className="icon-btn" style={{ width: 18, height: 18 }} onClick={() => setText("")}><X size={12} /></button>}
          </div>
          <button className={`btn ${s.showFilters || nFilters ? "on" : ""}`} onClick={() => setState({ showFilters: !s.showFilters })}>
            <SlidersHorizontal size={14} />Filter{nFilters ? ` (${nFilters})` : ""}
          </button>
          <select className="select" value={s.sort} onChange={(e) => setState({ sort: e.target.value as SortField })} title="Sort by">
            {SORTS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
          </select>
          <button className="icon-btn" title={s.descending ? "Newest / largest first" : "Oldest / smallest first"} onClick={() => setState({ descending: !s.descending })}>
            {s.descending ? <ArrowDownWideNarrow size={16} /> : <ArrowUpNarrowWide size={16} />}
          </button>
          <input type="range" min={110} max={420} step={10} value={s.thumbSize} onChange={(e) => setState({ thumbSize: Number(e.target.value) })} title="Thumbnail size" style={{ width: 90 }} />
          <button className={`icon-btn ${s.showInfoOverlay ? "on" : ""}`} title="Show names, dates and ratings under thumbnails" onClick={() => setState({ showInfoOverlay: !s.showInfoOverlay })}><Info size={16} /></button>
          <button className={`icon-btn ${s.autoAdvance ? "on" : ""}`} title="Auto-advance to the next item after rating or flagging" onClick={() => setState({ autoAdvance: !s.autoAdvance })}><FastForward size={16} /></button>
        </>
      )}
      <button className="btn primary" onClick={onImport}><FolderPlus size={14} />Add Media</button>
      {isGrid && <button className={`icon-btn ${s.rightOpen ? "on" : ""}`} title="Toggle details panel (⌘I)" onClick={() => setState({ rightOpen: !s.rightOpen })}><PanelRight size={16} /></button>}
    </div>
  );
}

const COLORS: ColorLabel[] = ["red", "orange", "yellow", "green", "blue", "purple"];

export function FilterBar({ tags, cameras }: { tags: Tag[]; cameras: string[] }) {
  const f = useStore((s) => s.filter);
  const view = useStore((s) => s.view);
  const set = (patch: Partial<MediaFilter>) => setState((s) => ({ filter: { ...s.filter, ...patch } }));
  const types = f.mediaTypes ?? [];
  const toggleType = (t: MediaType) => set({ mediaTypes: types.includes(t) ? types.filter((x) => x !== t) : [...types, t] });
  const num = (v: string) => (v === "" ? null : Number(v));

  const saveSmart = async () => {
    const rules = stripEmpty(effectiveFilter(view, f));
    if (!Object.keys(rules).length) { toast("Set some filters first", "error"); return; }
    const name = await promptText({ title: "Save as Smart Collection", label: "Name", placeholder: "e.g. Five-star GoPro videos", okLabel: "Save",
      hint: "Smart collections update automatically as media match these rules." });
    if (!name) return;
    try { const id = await api.createSmartCollection(name, rules); bumpStructure(); navigate({ kind: "collection", id }); } catch (e) { toast(errorText(e), "error"); }
  };

  return (
    <div className="filterbar" onKeyDown={(e) => e.stopPropagation()}>
      <div className="seg">
        {(["photo", "video", "audio"] as MediaType[]).map((t) => (
          <button key={t} className={types.includes(t) ? "on" : ""} onClick={() => toggleType(t)}>{t === "photo" ? "Photos" : t === "video" ? "Videos" : "Audio"}</button>
        ))}
      </div>
      <label>Rating
        <select className="select" value={f.unratedOnly ? "u" : f.minRating ?? ""} onChange={(e) => {
          const v = e.target.value;
          set(v === "u" ? { unratedOnly: true, minRating: null } : { unratedOnly: false, minRating: num(v) });
        }}>
          <option value="">Any</option>
          {[5, 4, 3, 2, 1].map((r) => <option key={r} value={r}>{"★".repeat(r)}{r < 5 ? " or more" : ""}</option>)}
          <option value="u">Unrated</option>
        </select>
      </label>
      <label>Status
        <select className="select" value={f.selectionStatus ?? ""} onChange={(e) => set({ selectionStatus: (e.target.value || null) as any })}>
          <option value="">Any</option>
          <option value="selected">Kept</option>
          <option value="rejected">Rejected</option>
          <option value="none">Not decided</option>
        </select>
      </label>
      <label><input type="checkbox" checked={!!f.favoriteOnly} onChange={(e) => set({ favoriteOnly: e.target.checked })} />Favorites</label>
      <label><input type="checkbox" checked={f.reviewStatus === "needs_review"} onChange={(e) => set({ reviewStatus: e.target.checked ? "needs_review" : null })} />Needs review</label>
      <label>Label
        <select className="select" value={f.colorLabel ?? ""} onChange={(e) => set({ colorLabel: (e.target.value || null) as ColorLabel | null })}>
          <option value="">Any</option>
          {COLORS.map((c) => <option key={c} value={c}>{c[0].toUpperCase() + c.slice(1)}</option>)}
        </select>
      </label>
      <label>Tag
        <select className="select" value={f.tagIds?.[0] ?? ""} onChange={(e) => set({ tagIds: e.target.value ? [Number(e.target.value)] : [] })} style={{ maxWidth: 180 }}>
          <option value="">Any</option>
          {tags.map((t) => <option key={t.id} value={t.id}>{t.path}</option>)}
        </select>
      </label>
      <label>From <input className="input" type="date" value={f.dateFrom ?? ""} onChange={(e) => set({ dateFrom: e.target.value || null })} /></label>
      <label>To <input className="input" type="date" value={f.dateTo ?? ""} onChange={(e) => set({ dateTo: e.target.value || null })} /></label>
      <label>Camera
        <input className="input" list="camera-list" placeholder="Any" value={f.camera ?? ""} onChange={(e) => set({ camera: e.target.value || null })} />
        <datalist id="camera-list">{cameras.map((c) => <option key={c} value={c} />)}</datalist>
      </label>
      <label>Duration (s)
        <input className="input narrow" type="number" min={0} placeholder="min" value={f.minDuration ?? ""} onChange={(e) => set({ minDuration: num(e.target.value) })} />
        –
        <input className="input narrow" type="number" min={0} placeholder="max" value={f.maxDuration ?? ""} onChange={(e) => set({ maxDuration: num(e.target.value) })} />
      </label>
      <label>Location
        <select className="select" value={f.hasLocation == null ? "" : f.hasLocation ? "y" : "n"} onChange={(e) => set({ hasLocation: e.target.value === "" ? null : e.target.value === "y" })}>
          <option value="">Any</option><option value="y">Has GPS</option><option value="n">No GPS</option>
        </select>
      </label>
      <span className="spacer" />
      <button className="btn small" onClick={saveSmart}><Sparkles size={13} />Save as Smart Collection</button>
      <button className="btn small ghost" onClick={() => setState({ filter: {} })}>Clear</button>
    </div>
  );
}

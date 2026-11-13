// Virtualized thumbnail grid: renders only visible rows, fetches pages lazily, and loads
// thumbnails through the `thumb://` protocol (generated on demand by the Rust core).
import { useEffect, useMemo, useRef, useState, memo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Ban, Check, Clock, Film, Flag, FolderInput, Heart, ImageOff, Loader2, Music, PenLine, TriangleAlert } from "lucide-react";
import { mediaList, useMediaList } from "../hooks/mediaList";
import { getState, setState, useStore } from "../store";
import { thumbUrl, api } from "../api";
import { Stars, openMenu, type MenuEntry } from "./ui";
import { formatDuration, formatShortDate } from "../format";
import { curate, addToCollection, targetIds, openFileDialog } from "../actions";
import type { MediaSummary, Collection } from "../types";

const GAP = 10;
const INFO_H = 44;

export function MediaGrid({ collections, emptyState }: { collections: Collection[]; emptyState: React.ReactNode }) {
  const list = useMediaList();
  const thumbSize = useStore((s) => s.thumbSize);
  const showInfo = useStore((s) => s.showInfoOverlay);
  const selection = useStore((s) => s.selection);
  const focusIndex = useStore((s) => s.focusIndex);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const inner = width - 24;
  const cols = Math.max(1, Math.floor((inner + GAP) / (thumbSize + GAP)));
  const tileW = Math.floor((inner - GAP * (cols - 1)) / cols);
  const tileH = Math.round(tileW * 0.78) + (showInfo ? INFO_H : 0);
  const rowCount = Math.ceil(list.total / cols);

  const virt = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => tileH + GAP,
    overscan: 3,
    paddingStart: 12,
    paddingEnd: 24,
  });
  useEffect(() => virt.measure(), [tileH, cols, virt]);

  const rows = virt.getVirtualItems();
  useEffect(() => {
    if (!rows.length) return;
    mediaList.ensureRange(rows[0].index * cols, (rows[rows.length - 1].index + 1) * cols + cols * 4);
  });

  // Keep the focused item visible during keyboard navigation.
  useEffect(() => {
    if (focusIndex >= 0) virt.scrollToIndex(Math.floor(focusIndex / cols), { align: "auto" });
  }, [focusIndex, cols, virt]);

  // Expose the column count for arrow-key navigation.
  useEffect(() => {
    (window as any).__gridCols = cols;
  }, [cols]);

  const onTileClick = (e: React.MouseEvent, index: number, item: MediaSummary) => {
    const s = getState();
    let sel = new Set(s.selection);
    if (e.shiftKey && s.anchorIndex >= 0) {
      const [a, b] = [Math.min(s.anchorIndex, index), Math.max(s.anchorIndex, index)];
      if (!(e.metaKey || e.ctrlKey)) sel = new Set();
      for (let i = a; i <= b; i++) {
        const it = mediaList.get(i);
        if (it) sel.add(it.id);
      }
      setState({ selection: sel, focusIndex: index });
    } else if (e.metaKey || e.ctrlKey) {
      if (sel.has(item.id)) sel.delete(item.id);
      else sel.add(item.id);
      setState({ selection: sel, focusIndex: index, anchorIndex: index });
    } else {
      setState({ selection: new Set([item.id]), focusIndex: index, anchorIndex: index });
    }
    scrollRef.current?.focus();
  };

  const onContext = (e: React.MouseEvent, index: number, item: MediaSummary) => {
    const s = getState();
    if (!s.selection.has(item.id)) setState({ selection: new Set([item.id]), focusIndex: index, anchorIndex: index });
    const ids = targetIds(item.id);
    const manual = collections.filter((c) => c.collectionType === "manual");
    const n = ids.length;
    const items: MenuEntry[] = [
      { label: "Open", onClick: () => setState({ viewerId: item.id }), shortcut: "Space" },
      { separator: true },
      ...[5, 4, 3, 2, 1, 0].map((r) => ({ label: r ? "★".repeat(r) : "Clear rating", onClick: () => curate(ids, { rating: r }), shortcut: String(r) })),
      { separator: true },
      { label: "Mark as Keep", icon: <Check size={14} />, onClick: () => curate(ids, { selectionStatus: "selected" }), shortcut: "P" },
      { label: "Reject", icon: <Ban size={14} />, onClick: () => curate(ids, { selectionStatus: "rejected" }), shortcut: "X" },
      { label: "Clear Keep / Reject", onClick: () => curate(ids, { selectionStatus: "none" }), shortcut: "U" },
      { label: "Needs Review", icon: <Flag size={14} />, onClick: () => curate(ids, { reviewStatus: "needs_review" }), shortcut: "R" },
      { label: item.favorite ? "Remove from Favorites" : "Add to Favorites", icon: <Heart size={14} />, onClick: () => curate(ids, { favorite: !item.favorite }), shortcut: "F" },
    ];
    if (manual.length) {
      items.push({ separator: true }, { title: n > 1 ? `Add ${n} items to collection` : "Add to collection" });
      manual.slice(0, 12).forEach((c) => items.push({ label: c.name, onClick: () => addToCollection(c.id, ids) }));
    }
    const view = s.view;
    if (view.kind === "collection" && collections.find((c) => c.id === view.id)?.collectionType === "manual") {
      items.push({
        label: "Remove from this collection",
        danger: true,
        onClick: async () => {
          await api.removeFromCollection(view.id, ids);
          setState((st) => ({ revision: st.revision + 1, structureRevision: st.structureRevision + 1, selection: new Set() }));
        },
      });
    }
    items.push(
      { separator: true },
      { label: n > 1 ? `Rename ${n} Files…` : "Rename File…", icon: <PenLine size={14} />, onClick: () => openFileDialog("rename", ids), shortcut: "F2" },
      { label: "Copy or Move to Folder…", icon: <FolderInput size={14} />, onClick: () => openFileDialog("organize", ids) },
      { separator: true },
      { label: "Show in Finder", onClick: () => api.revealInFinder(item.id) },
      { label: "Open with Default App", onClick: () => api.openExternal(item.id) },
    );
    openMenu(e, items);
  };

  if (list.loaded && list.total === 0) return <>{emptyState}</>;

  return (
    <div
      ref={scrollRef}
      className="grid-scroll"
      tabIndex={0}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) setState({ selection: new Set(), focusIndex: -1 });
      }}
    >
      <div style={{ height: virt.getTotalSize(), position: "relative" }}>
        {rows.map((row) => (
          <div key={row.key} className="grid-row" style={{ top: row.start, height: tileH }}>
            {Array.from({ length: cols }, (_, c) => {
              const index = row.index * cols + c;
              if (index >= list.total) return null;
              const item = list.get(index);
              return (
                <div key={c} style={{ width: tileW, height: tileH, flexShrink: 0 }}>
                  {item ? (
                    <Tile
                      item={item}
                      selected={selection.has(item.id)}
                      focused={focusIndex === index}
                      showInfo={showInfo}
                      onClick={(e) => onTileClick(e, index, item)}
                      onDoubleClick={() => setState({ viewerId: item.id, focusIndex: index })}
                      onContextMenu={(e) => onContext(e, index, item)}
                    />
                  ) : (
                    <div className="tile" style={{ height: "100%" }}>
                      <div className="thumb" />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

const Tile = memo(function Tile({
  item,
  selected,
  focused,
  showInfo,
  onClick,
  onDoubleClick,
  onContextMenu,
}: {
  item: MediaSummary;
  selected: boolean;
  focused: boolean;
  showInfo: boolean;
  onClick: (e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  const [failed, setFailed] = useState(false);
  const [loaded, setLoaded] = useState(false);
  // Changing status (pending → done) re-requests a thumbnail that previously failed.
  const src = useMemo(() => thumbUrl(item.id, item.thumbnailStatus), [item.id, item.thumbnailStatus]);
  useEffect(() => setFailed(false), [src]);
  const cls = ["tile", selected && "selected", focused && "focused", item.selectionStatus === "rejected" && "rejected", item.isMissing && "missing"].filter(Boolean).join(" ");

  const onDragStart = (e: React.DragEvent) => {
    const ids = targetIds(item.id);
    e.dataTransfer.setData("application/x-media-ids", JSON.stringify(ids));
    e.dataTransfer.effectAllowed = "copy";
    const ghost = document.createElement("div");
    ghost.textContent = ids.length > 1 ? `${ids.length} items` : item.filename;
    ghost.style.cssText = "position:fixed;top:-100px;padding:6px 10px;background:#3a7bff;color:#fff;border-radius:6px;font:12px -apple-system";
    document.body.appendChild(ghost);
    e.dataTransfer.setDragImage(ghost, 10, 10);
    setTimeout(() => ghost.remove(), 0);
  };

  return (
    <div className={cls} style={{ height: "100%" }} onClick={onClick} onDoubleClick={onDoubleClick} onContextMenu={onContextMenu} draggable onDragStart={onDragStart} title={item.filename}>
      <div className="thumb">
        {item.mediaType === "audio" && (failed || item.thumbnailStatus === "unsupported") ? (
          <div className="placeholder"><Music size={34} /><span>Audio</span></div>
        ) : failed ? (
          <div className="placeholder">
            {item.thumbnailStatus === "pending" ? <Loader2 size={22} className="spin" /> : <ImageOff size={26} />}
            <span>{item.thumbnailStatus === "pending" ? "Preparing…" : "No preview"}</span>
          </div>
        ) : (
          <img src={src} loading="lazy" decoding="async" draggable={false} onLoad={() => setLoaded(true)} onError={() => setFailed(true)} style={{ opacity: loaded ? 1 : 0, transition: "opacity .15s" }} alt="" />
        )}
        {item.mediaType === "video" && (
          <span className="badge br"><Film size={11} />{formatDuration(item.duration)}</span>
        )}
        {item.mediaType === "audio" && item.duration != null && <span className="badge br"><Music size={11} />{formatDuration(item.duration)}</span>}
        {item.selectionStatus === "selected" && <span className="badge tl pick"><Check size={11} />Keep</span>}
        {item.selectionStatus === "rejected" && <span className="badge tl reject"><Ban size={11} />Reject</span>}
        {item.reviewStatus === "needs_review" && item.selectionStatus === "none" && <span className="badge tl review"><Clock size={11} />Review</span>}
        {item.favorite && <span className="badge tr"><Heart size={11} fill="#ff6b8b" color="#ff6b8b" /></span>}
        {item.isMissing && <span className="badge bl missing"><TriangleAlert size={11} />Missing</span>}
        {item.colorLabel && <div className={`color-bar label-${item.colorLabel}`} />}
      </div>
      {showInfo && (
        <div className="tile-info">
          <div className="tile-name">{item.filename}</div>
          <div className="tile-meta">
            <span className="date" title={item.captureKnown ? undefined : "Capture date unknown — sorted by file date"}>{item.captureKnown ? formatShortDate(item.sortDatetime) : item.mediaType === "audio" ? "" : "No date"}</span>
            {item.rating > 0 ? <Stars value={item.rating} size={10} /> : <span />}
          </div>
        </div>
      )}
    </div>
  );
});

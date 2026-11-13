// Minimal global store (useSyncExternalStore) for UI state shared across panels.
import { useSyncExternalStore } from "react";
import type { MediaFilter, SortField, View, ActiveJob } from "./types";

export interface UIState {
  view: View;
  /** User-added filters layered on top of the view's own scope. */
  filter: MediaFilter;
  sort: SortField;
  descending: boolean;
  thumbSize: number;
  selection: Set<number>;
  /** Index (in the current result list) of the focused item. */
  focusIndex: number;
  anchorIndex: number;
  /** Item id shown in the large viewer, or null. */
  viewerId: number | null;
  leftOpen: boolean;
  rightOpen: boolean;
  showFilters: boolean;
  autoAdvance: boolean;
  /** Bumped when the catalog changes, so lists refetch. */
  revision: number;
  /** Bumped when sidebar structures (tags/collections/sources) change. */
  structureRevision: number;
  jobs: ActiveJob[];
  theme: "dark" | "light" | "system";
  showInfoOverlay: boolean;
  /** Open file-operation dialog, if any. */
  dialog: { kind: "rename" | "organize"; ids: number[] } | null;
  /** Clip editor visible in the video viewer. */
  clipMode: boolean;
}

const saved = (() => {
  try {
    return JSON.parse(localStorage.getItem("mc-ui") || "{}");
  } catch {
    return {};
  }
})();

let state: UIState = {
  view: { kind: "dashboard" },
  filter: {},
  sort: saved.sort ?? "captureDate",
  descending: saved.descending ?? true,
  thumbSize: saved.thumbSize ?? 190,
  selection: new Set(),
  focusIndex: -1,
  anchorIndex: -1,
  viewerId: null,
  leftOpen: saved.leftOpen ?? true,
  rightOpen: saved.rightOpen ?? true,
  showFilters: false,
  autoAdvance: saved.autoAdvance ?? false,
  revision: 0,
  structureRevision: 0,
  jobs: [],
  theme: saved.theme ?? "dark",
  showInfoOverlay: saved.showInfoOverlay ?? true,
  dialog: null,
  clipMode: false,
};

const listeners = new Set<() => void>();

export function getState() {
  return state;
}

export function setState(patch: Partial<UIState> | ((s: UIState) => Partial<UIState>)) {
  const p = typeof patch === "function" ? patch(state) : patch;
  state = { ...state, ...p };
  const { sort, descending, thumbSize, leftOpen, rightOpen, autoAdvance, theme, showInfoOverlay } = state;
  localStorage.setItem("mc-ui", JSON.stringify({ sort, descending, thumbSize, leftOpen, rightOpen, autoAdvance, theme, showInfoOverlay }));
  listeners.forEach((l) => l());
}

export function useStore<T>(selector: (s: UIState) => T): T {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => selector(state),
  );
}

export function navigate(view: View, filter: MediaFilter = {}) {
  setState({ view, filter, showFilters: Object.keys(filter).length > 0, selection: new Set(), focusIndex: -1, anchorIndex: -1, viewerId: null });
}

export const bump = () => setState((s) => ({ revision: s.revision + 1 }));
export const bumpStructure = () => setState((s) => ({ structureRevision: s.structureRevision + 1, revision: s.revision + 1 }));

/** Filter defining the current view's scope. */
export function viewFilter(view: View): MediaFilter | null {
  switch (view.kind) {
    case "library":
      switch (view.preset) {
        case "all": return {};
        case "photos": return { mediaTypes: ["photo"] };
        case "videos": return { mediaTypes: ["video"] };
        case "audio": return { mediaTypes: ["audio"] };
        case "unrated": return { unratedOnly: true };
        case "favorites": return { favoriteOnly: true };
        case "picks": return { selectionStatus: "selected" };
        case "rejected": return { selectionStatus: "rejected" };
        case "review": return { reviewStatus: "needs_review" };
        case "recent": return { addedSinceMs: Date.now() - 7 * 24 * 3600 * 1000 };
        case "missing": return { missingOnly: true };
        case "nodate": return { noCaptureDate: true };
      }
      return {};
    case "collection": return { collectionId: view.id };
    case "tag": return { tagIds: [view.id] };
    case "folder": return { folder: view.path, sourceId: view.sourceId };
    default: return null;
  }
}

/** View scope + user filters, merged (arrays concatenate). */
export function effectiveFilter(view: View, user: MediaFilter): MediaFilter {
  const base = viewFilter(view) ?? {};
  const merged: MediaFilter = { ...base, ...stripEmpty(user) };
  if (base.tagIds?.length || user.tagIds?.length) merged.tagIds = [...(base.tagIds ?? []), ...(user.tagIds ?? [])];
  if (base.mediaTypes?.length && user.mediaTypes?.length) merged.mediaTypes = base.mediaTypes.filter((t) => user.mediaTypes!.includes(t));
  return merged;
}

export function stripEmpty(f: MediaFilter): MediaFilter {
  const out: any = {};
  for (const [k, v] of Object.entries(f)) {
    if (v === null || v === undefined || v === "" || v === false) continue;
    if (Array.isArray(v) && v.length === 0) continue;
    out[k] = v;
  }
  return out;
}

export function activeFilterCount(f: MediaFilter): number {
  return Object.keys(stripEmpty({ ...f, text: null })).length;
}

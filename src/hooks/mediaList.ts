// Sparse, paged view over the current query result. Only pages that are (or were) on
// screen are fetched, so a 50,000-item library costs a few hundred rows of memory.
import { useEffect, useSyncExternalStore } from "react";
import { api, errorText } from "../api";
import type { MediaFilter, MediaSummary, SortField } from "../types";

const PAGE = 240;

class MediaList {
  key = "";
  filter: MediaFilter = {};
  sort: SortField = "captureDate";
  descending = true;
  total = 0;
  loaded = false;
  error: string | null = null;
  pages = new Map<number, MediaSummary[]>();
  inflight = new Map<number, number>(); // page -> generation
  generation = 0;
  version = 0;
  listeners = new Set<() => void>();

  emit() {
    this.version++;
    this.listeners.forEach((l) => l());
  }

  setQuery(filter: MediaFilter, sort: SortField, descending: boolean) {
    const key = JSON.stringify([filter, sort, descending]);
    if (key === this.key) return;
    this.key = key;
    this.filter = filter;
    this.sort = sort;
    this.descending = descending;
    this.generation++;
    this.pages.clear();
    this.inflight.clear();
    this.loaded = false;
    this.total = 0;
    this.fetchPage(0);
  }

  /** Re-fetch every loaded page in place (no flicker) after the catalog changed. */
  refresh() {
    this.generation++;
    const pages = this.pages.size ? [...this.pages.keys()] : [0];
    this.inflight.clear();
    pages.forEach((p) => this.fetchPage(p, true));
  }

  async fetchPage(page: number, keepOld = false) {
    if (this.inflight.get(page) === this.generation) return;
    const gen = this.generation;
    this.inflight.set(page, gen);
    try {
      const res = await api.queryMedia({ filter: this.filter, sort: this.sort, descending: this.descending, offset: page * PAGE, limit: PAGE });
      if (gen !== this.generation) return;
      this.total = res.total;
      this.loaded = true;
      this.error = null;
      this.pages.set(page, res.items);
      // Drop pages now beyond the end.
      for (const p of [...this.pages.keys()]) if (p * PAGE >= res.total && p !== 0) this.pages.delete(p);
      this.emit();
    } catch (e) {
      if (!keepOld) this.error = errorText(e);
      this.emit();
    } finally {
      if (this.inflight.get(page) === gen) this.inflight.delete(page);
    }
  }

  ensureRange(start: number, end: number) {
    if (end < start) return;
    const p0 = Math.floor(Math.max(0, start) / PAGE);
    const p1 = Math.floor(Math.max(0, end) / PAGE);
    for (let p = p0; p <= p1; p++) if (!this.pages.has(p)) this.fetchPage(p);
  }

  get(index: number): MediaSummary | undefined {
    return this.pages.get(Math.floor(index / PAGE))?.[index % PAGE];
  }

  indexOf(id: number): number {
    for (const [p, items] of this.pages) {
      const i = items.findIndex((x) => x.id === id);
      if (i >= 0) return p * PAGE + i;
    }
    return -1;
  }

  /** Resolve an item, fetching its page if needed. */
  async getAsync(index: number): Promise<MediaSummary | undefined> {
    if (index < 0 || index >= this.total) return undefined;
    const it = this.get(index);
    if (it) return it;
    await this.fetchPage(Math.floor(index / PAGE));
    return this.get(index);
  }
}

export const mediaList = new MediaList();

export function useMediaList() {
  useSyncExternalStore(
    (l) => {
      mediaList.listeners.add(l);
      return () => mediaList.listeners.delete(l);
    },
    () => mediaList.version,
  );
  return mediaList;
}

export function useMediaQuerySync(filter: MediaFilter | null, sort: SortField, descending: boolean, revision: number) {
  const key = JSON.stringify([filter, sort, descending]);
  useEffect(() => {
    if (filter) mediaList.setQuery(filter, sort, descending);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  useEffect(() => {
    if (revision > 0) mediaList.refresh();
  }, [revision]);
}

import type {
  Coord,
  MultiRouteResponse,
  NewPosition,
  Position,
  Stats,
} from "./types";

const STORAGE_KEY = "moncap.apiBase";

/** URL du backend, persistée dans le navigateur (modifiable dans l'UI). */
export function getApiBase(): string {
  return localStorage.getItem(STORAGE_KEY) ?? "http://localhost:3000";
}

export function setApiBase(url: string): void {
  localStorage.setItem(STORAGE_KEY, url.replace(/\/+$/, ""));
}

async function json<T>(res: Response): Promise<T> {
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as T;
}

export const api = {
  positions: (): Promise<Position[]> =>
    fetch(`${getApiBase()}/positions`).then(json<Position[]>),

  add: (p: NewPosition): Promise<Position> =>
    fetch(`${getApiBase()}/positions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(p),
    }).then(json<Position>),

  update: (id: number, p: NewPosition): Promise<Position> =>
    fetch(`${getApiBase()}/positions/${id}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(p),
    }).then(json<Position>),

  remove: async (id: number): Promise<void> => {
    await fetch(`${getApiBase()}/positions/${id}`, { method: "DELETE" });
  },

  multiRoute: (points: Coord[], speedKmh: number): Promise<MultiRouteResponse> =>
    fetch(`${getApiBase()}/route/multi`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ points, speed_kmh: speedKmh }),
    }).then(json<MultiRouteResponse>),

  stats: (): Promise<Stats> =>
    fetch(`${getApiBase()}/stats`).then(json<Stats>),

  importGpx: (gpx: string): Promise<Position[]> =>
    fetch(`${getApiBase()}/positions/import`, {
      method: "POST",
      headers: { "content-type": "application/gpx+xml" },
      body: gpx,
    }).then(json<Position[]>),

  gpxUrl: (): string => `${getApiBase()}/positions.gpx`,
};

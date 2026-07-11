import type {
  Coord,
  MultiRouteResponse,
  NewPosition,
  Position,
  Stats,
} from "./types";

const BASE_KEY = "moncap.apiBase";
const TOKEN_KEY = "moncap.token";
const USER_KEY = "moncap.user";

/**
 * URL du backend, persistée dans le navigateur (modifiable dans l'UI).
 * Vide par défaut = même origine (cas où l'API sert aussi le front, ex. Heroku).
 */
export function getApiBase(): string {
  return localStorage.getItem(BASE_KEY) ?? "";
}

export function setApiBase(url: string): void {
  localStorage.setItem(BASE_KEY, url.replace(/\/+$/, ""));
}

// --- Session ---

export function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? "";
}

export function getUsername(): string {
  return localStorage.getItem(USER_KEY) ?? "";
}

function setSession(token: string, username: string): void {
  localStorage.setItem(TOKEN_KEY, token);
  localStorage.setItem(USER_KEY, username);
}

export function logout(): void {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
}

/** Erreur levée sur réponse 401 (session expirée/invalide). */
export class Unauthorized extends Error {}

function authHeaders(extra: Record<string, string> = {}): Record<string, string> {
  const t = getToken();
  return t ? { ...extra, authorization: `Bearer ${t}` } : extra;
}

async function json<T>(res: Response): Promise<T> {
  if (res.status === 401) throw new Unauthorized();
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as T;
}

// --- Authentification ---

interface AuthResponse {
  token: string;
  username: string;
}

async function auth(path: string, username: string, password: string): Promise<void> {
  const res = await fetch(`${getApiBase()}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) {
    throw new Error((await res.text()) || `HTTP ${res.status}`);
  }
  const data = (await res.json()) as AuthResponse;
  setSession(data.token, data.username);
}

export const signup = (u: string, p: string) => auth("/auth/signup", u, p);
export const login = (u: string, p: string) => auth("/auth/login", u, p);

// --- API positions / itinéraires ---

export const api = {
  positions: (): Promise<Position[]> =>
    fetch(`${getApiBase()}/positions`, { headers: authHeaders() }).then(json<Position[]>),

  add: (p: NewPosition): Promise<Position> =>
    fetch(`${getApiBase()}/positions`, {
      method: "POST",
      headers: authHeaders({ "content-type": "application/json" }),
      body: JSON.stringify(p),
    }).then(json<Position>),

  update: (id: number, p: NewPosition): Promise<Position> =>
    fetch(`${getApiBase()}/positions/${id}`, {
      method: "PUT",
      headers: authHeaders({ "content-type": "application/json" }),
      body: JSON.stringify(p),
    }).then(json<Position>),

  remove: async (id: number): Promise<void> => {
    await fetch(`${getApiBase()}/positions/${id}`, {
      method: "DELETE",
      headers: authHeaders(),
    });
  },

  multiRoute: (points: Coord[], speedKmh: number): Promise<MultiRouteResponse> =>
    fetch(`${getApiBase()}/route/multi`, {
      method: "POST",
      headers: authHeaders({ "content-type": "application/json" }),
      body: JSON.stringify({ points, speed_kmh: speedKmh }),
    }).then(json<MultiRouteResponse>),

  stats: (): Promise<Stats> =>
    fetch(`${getApiBase()}/stats`, { headers: authHeaders() }).then(json<Stats>),

  importGpx: (gpx: string): Promise<Position[]> =>
    fetch(`${getApiBase()}/positions/import`, {
      method: "POST",
      headers: authHeaders({ "content-type": "application/gpx+xml" }),
      body: gpx,
    }).then(json<Position[]>),

  // Vote sur un signalement : « toujours là » (up) ou « plus là ».
  voteAlert: async (id: number, up: boolean): Promise<void> => {
    await fetch(`${getApiBase()}/alerts/${id}/vote`, {
      method: "POST",
      headers: authHeaders({ "content-type": "application/json" }),
      body: JSON.stringify({ up }),
    });
  },

  // Téléchargé via un lien : le jeton passe en query.
  gpxUrl: (): string =>
    `${getApiBase()}/positions.gpx?token=${encodeURIComponent(getToken())}`,
};

/** URL WebSocket (jeton en query : les WebSocket ne portent pas d'en-tête). */
export function wsUrl(): string {
  const token = encodeURIComponent(getToken());
  const base = getApiBase();
  if (!base) {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    return `${proto}://${location.host}/ws?token=${token}`;
  }
  return `${base.replace(/^http/, "ws")}/ws?token=${token}`;
}

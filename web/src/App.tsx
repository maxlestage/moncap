import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  getApiBase,
  getToken,
  getUsername,
  logout,
  Unauthorized,
  wsUrl,
} from "./api";
import { AuthView } from "./AuthView";
import { AVATARS, avatarUrl, getAvatar, setAvatar } from "./avatars";
import { MapView } from "./MapView";
import type { Alert, Coord, LiveUser, Position, Stats } from "./types";

const ALERT_TYPES = [
  { category: "police", emoji: "🚓", label: "Police" },
  { category: "accident", emoji: "💥", label: "Accident" },
  { category: "bouchon", emoji: "🚧", label: "Bouchon" },
  { category: "danger", emoji: "⚠️", label: "Danger" },
  { category: "vehicule", emoji: "🚘", label: "Véhicule arrêté" },
  { category: "objet", emoji: "📦", label: "Objet sur la route" },
  { category: "travaux", emoji: "🏗️", label: "Travaux" },
  { category: "brouillard", emoji: "🌫️", label: "Brouillard" },
];

const LIVE_TTL = 15_000; // une voiture live disparaît après 15 s sans nouvelle
const ALERT_TTL = 30 * 60_000; // un signalement expire après 30 min

/** Une destination choisie (nom + coordonnées). */
interface Destination {
  lat: number;
  lon: number;
  label: string;
}

/** Résultat de géocodage renvoyé par Nominatim (OpenStreetMap). */
interface GeoResult {
  display_name: string;
  lat: string;
  lon: string;
}

/** Distance en km entre deux points (formule de Haversine). */
function haversineKm(a: Coord, b: Coord): number {
  const R = 6371;
  const dLat = ((b.lat - a.lat) * Math.PI) / 180;
  const dLon = ((b.lon - a.lon) * Math.PI) / 180;
  const la1 = (a.lat * Math.PI) / 180;
  const la2 = (b.lat * Math.PI) / 180;
  const h =
    Math.sin(dLat / 2) ** 2 + Math.cos(la1) * Math.cos(la2) * Math.sin(dLon / 2) ** 2;
  return 2 * R * Math.asin(Math.sqrt(h));
}

/** Point cardinal (français) du cap entre deux points. */
function compass(a: Coord, b: Coord): string {
  const dLon = ((b.lon - a.lon) * Math.PI) / 180;
  const la1 = (a.lat * Math.PI) / 180;
  const la2 = (b.lat * Math.PI) / 180;
  const y = Math.sin(dLon) * Math.cos(la2);
  const x = Math.cos(la1) * Math.sin(la2) - Math.sin(la1) * Math.cos(la2) * Math.cos(dLon);
  const deg = ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
  const dirs = ["Nord", "Nord-Est", "Est", "Sud-Est", "Sud", "Sud-Ouest", "Ouest", "Nord-Ouest"];
  return dirs[Math.round(deg / 45) % 8];
}

/** Géocode une adresse/lieu via Nominatim (OpenStreetMap), priorité France. */
async function geocode(query: string): Promise<GeoResult[]> {
  const url =
    "https://nominatim.openstreetmap.org/search?format=json&limit=5&accept-language=fr" +
    `&countrycodes=fr&q=${encodeURIComponent(query)}`;
  const res = await fetch(url, { headers: { "Accept-Language": "fr" } });
  if (!res.ok) return [];
  return (await res.json()) as GeoResult[];
}

export function App() {
  const [authed, setAuthed] = useState(!!getToken());
  if (!authed) {
    return <AuthView onAuthed={() => setAuthed(true)} />;
  }
  return <MapApp onLogout={() => setAuthed(false)} />;
}

function MapApp({ onLogout }: { onLogout: () => void }) {
  const [positions, setPositions] = useState<Position[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [routeInfo, setRouteInfo] = useState("");
  const [speed, setSpeed] = useState(50);
  const [error, setError] = useState("");
  const [connected, setConnected] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [avatar, setAvatarState] = useState(getAvatar());
  const [liveUsers, setLiveUsers] = useState<Record<number, LiveUser>>({});
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [me, setMe] = useState<Coord | null>(null);
  const [destQuery, setDestQuery] = useState("");
  const [destResults, setDestResults] = useState<GeoResult[]>([]);
  const [destination, setDestination] = useState<Destination | null>(null);

  const wsRef = useRef<WebSocket | null>(null);
  const myPos = useRef<Coord | null>(null);
  const watchId = useRef<number | null>(null);
  // Dernier envoi de position live (throttle du WebSocket de partage).
  const lastLive = useRef<{ t: number; c: Coord } | null>(null);

  const signOut = useCallback(() => {
    logout();
    onLogout();
  }, [onLogout]);

  const refresh = useCallback(async () => {
    try {
      setError("");
      const [pos, st] = await Promise.all([api.positions(), api.stats()]);
      setPositions(pos);
      setStats(st);
    } catch (e) {
      if (e instanceof Unauthorized) {
        signOut();
      } else {
        setError(`Impossible de joindre l'API (${getApiBase() || "même serveur"}).`);
      }
    }
  }, [signOut]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Connexion WebSocket temps réel (reconnecte si l'URL d'API change).
  useEffect(() => {
    let closedByUs = false;
    let retry = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const handle = (e: MessageEvent) => {
      const ev = JSON.parse(e.data as string);
      switch (ev.kind) {
        case "positions_changed":
          refresh();
          break;
        case "live":
          setLiveUsers((prev) => ({
            ...prev,
            [ev.id]: {
              id: ev.id,
              lat: ev.lat,
              lon: ev.lon,
              label: ev.label,
              avatar: ev.avatar ?? "green",
              ts: Date.now(),
            },
          }));
          break;
        case "live_gone":
          setLiveUsers((prev) => {
            const next = { ...prev };
            delete next[ev.id];
            return next;
          });
          break;
        case "alert":
          setAlerts((prev) => [...prev.filter((a) => a.id !== ev.id), ev]);
          break;
        case "alerts":
          setAlerts(ev.alerts);
          break;
      }
    };

    // Connexion avec reconnexion automatique (backoff exponentiel, max 15 s).
    const connect = () => {
      const ws = new WebSocket(wsUrl());
      wsRef.current = ws;
      ws.onopen = () => {
        retry = 0;
        setConnected(true);
      };
      ws.onmessage = handle;
      ws.onerror = () => ws.close();
      ws.onclose = () => {
        setConnected(false);
        if (closedByUs) return;
        const delay = Math.min(1000 * 2 ** retry, 15000);
        retry += 1;
        timer = setTimeout(connect, delay);
      };
    };
    connect();

    return () => {
      closedByUs = true;
      if (timer) clearTimeout(timer);
      wsRef.current?.close();
    };
  }, [refresh]);

  // Purge des voitures live et signalements expirés.
  useEffect(() => {
    const t = setInterval(() => {
      const now = Date.now();
      setLiveUsers((prev) => {
        const next: Record<number, LiveUser> = {};
        for (const u of Object.values(prev)) if (now - u.ts < LIVE_TTL) next[u.id] = u;
        return next;
      });
      setAlerts((prev) => prev.filter((a) => now - a.ts < ALERT_TTL));
    }, 3000);
    return () => clearInterval(t);
  }, []);

  // Recherche de destination (anti-rebond 350 ms) via géocodage Nominatim.
  useEffect(() => {
    const q = destQuery.trim();
    if (q.length < 3) {
      setDestResults([]);
      return;
    }
    let cancelled = false;
    const t = setTimeout(async () => {
      try {
        const results = await geocode(q);
        if (!cancelled) setDestResults(results);
      } catch {
        if (!cancelled) setDestResults([]);
      }
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [destQuery]);

  // Choisit une destination parmi les résultats de recherche.
  const chooseDestination = (r: GeoResult) => {
    setDestination({
      lat: Number(r.lat),
      lon: Number(r.lon),
      label: r.display_name,
    });
    setDestResults([]);
    setDestQuery("");
  };

  const send = (msg: unknown) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(msg));
  };

  // Démarre le partage de ma position en direct. Nom affiché = mon e-mail de
  // connexion (sauf si on en fournit un autre).
  const startSharing = useCallback(
    (label: string = getUsername() || "Moi") => {
      if (!navigator.geolocation || watchId.current != null) return;
      watchId.current = navigator.geolocation.watchPosition(
        (p) => {
          const c = { lat: p.coords.latitude, lon: p.coords.longitude };
          myPos.current = c;
          setMe(c);
          // Throttle : au plus 1 envoi / 1,5 s et si on a bougé d'au moins ~8 m,
          // pour ne pas inonder le WebSocket à la cadence du GPS.
          const prev = lastLive.current;
          const dLat = prev ? (c.lat - prev.c.lat) * 111320 : Infinity;
          const dLon = prev
            ? (c.lon - prev.c.lon) * 111320 * Math.cos((c.lat * Math.PI) / 180)
            : Infinity;
          const moved = Math.hypot(dLat, dLon);
          if (!prev || (Date.now() - prev.t >= 1500 && moved >= 8)) {
            lastLive.current = { t: Date.now(), c };
            send({ kind: "live", lat: c.lat, lon: c.lon, label, avatar });
          }
        },
        () => setError("Partage de position refusé."),
        { enableHighAccuracy: true, maximumAge: 2000 },
      );
      setSharing(true);
    },
    [avatar],
  );

  // Active/désactive le partage de ma position en direct.
  const toggleSharing = () => {
    if (sharing) {
      if (watchId.current != null) navigator.geolocation.clearWatch(watchId.current);
      watchId.current = null;
      setSharing(false);
      return;
    }
    if (!navigator.geolocation) {
      setError("Géolocalisation indisponible.");
      return;
    }
    startSharing();
  };

  // Partage automatique au chargement : tout le monde apparaît sur la carte.
  useEffect(() => {
    startSharing();
    return () => {
      if (watchId.current != null) navigator.geolocation.clearWatch(watchId.current);
      watchId.current = null;
    };
    // Ne s'exécute qu'une fois au montage.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Envoie un signalement à ma position courante.
  const report = (category: string, label: string) => {
    const fire = (c: Coord) => send({ kind: "alert", category, lat: c.lat, lon: c.lon, label });
    if (myPos.current) {
      fire(myPos.current);
    } else if (navigator.geolocation) {
      navigator.geolocation.getCurrentPosition(
        (p) => fire({ lat: p.coords.latitude, lon: p.coords.longitude }),
        () => setError("Position requise pour signaler."),
      );
    } else {
      setError("Géolocalisation indisponible.");
    }
  };

  // Vote sur un signalement ; la mise à jour revient par le WebSocket.
  const voteAlert = useCallback((id: number, up: boolean) => {
    api.voteAlert(id, up).catch(() => setError("Vote impossible."));
  }, []);

  const addPoint = useCallback(
    async (coord: Coord) => {
      const label = prompt("Nom de la position ?", "Point") ?? "Point";
      try {
        await api.add({ lat: coord.lat, lon: coord.lon, label });
      } catch {
        setError("Échec de l'ajout.");
      }
    },
    [],
  );

  const remove = async (id: number) => {
    await api.remove(id);
  };

  const rename = async (p: Position) => {
    const label = prompt("Nouveau nom ?", p.label);
    if (label == null) return;
    await api.update(p.id, { lat: p.lat, lon: p.lon, label });
  };

  const computeRoute = async () => {
    if (positions.length < 2) return;
    const r = await api.multiRoute(
      positions.map((p) => ({ lat: p.lat, lon: p.lon })),
      speed,
    );
    setRouteInfo(
      `Itinéraire : ${r.total_km.toFixed(1)} km · ${r.duration_min.toFixed(0)} min à ${speed} km/h`,
    );
  };

  const importGpx = async (file: File) => {
    await api.importGpx(await file.text());
  };

  const liveList = Object.values(liveUsers);

  return (
    <div className="app">
      <header>
        <h1>
          🛰️ MonCap GPS{" "}
          <span className={`dot ${connected ? "on" : "off"}`} title={connected ? "Temps réel connecté" : "Déconnecté"} />
        </h1>
        <div className="apibar">
          <span className="user">
            <img className="user-avatar" src={avatarUrl(avatar)} alt="" /> {getUsername()}
          </span>
          <button onClick={signOut}>Déconnexion</button>
        </div>
      </header>

      <div className="dest">
        <div className="dest-search">
          <span className="dest-icon">🔎</span>
          <input
            type="text"
            placeholder="Où allez-vous ? (adresse ou lieu)"
            value={destQuery}
            onChange={(e) => setDestQuery(e.target.value)}
          />
          {destination && (
            <button className="dest-clear" title="Effacer" onClick={() => setDestination(null)}>
              ✕
            </button>
          )}
        </div>
        {destResults.length > 0 && (
          <ul className="dest-results">
            {destResults.map((r, i) => (
              <li key={i}>
                <button onClick={() => chooseDestination(r)}>
                  📍 {r.display_name}
                </button>
              </li>
            ))}
          </ul>
        )}
        {destination && (
          <div className="dest-banner">
            🎯 <strong>Vers :</strong> {destination.label.split(",").slice(0, 3).join(",")}
            {me && (
              <span className="dest-meta">
                {" · "}
                {haversineKm(me, destination).toFixed(1)} km · direction {compass(me, destination)}
              </span>
            )}
          </div>
        )}
      </div>

      {error && <div className="error">{error}</div>}

      <MapView
        positions={positions}
        liveUsers={liveList}
        alerts={alerts}
        me={me}
        destination={destination}
        onAddPoint={addPoint}
        onVoteAlert={voteAlert}
      />

      <section className="controls">
        <p className="hint">Astuce : clique sur la carte pour ajouter une position.</p>

        <div className="row avatars">
          <span className="avatars-label">Mon avatar :</span>
          {AVATARS.map((a) => (
            <button
              key={a.id}
              className={`avatar-pick ${avatar === a.id ? "selected" : ""}`}
              title={a.label}
              onClick={() => {
                setAvatar(a.id);
                setAvatarState(a.id);
              }}
            >
              <img src={a.url} alt={a.label} />
            </button>
          ))}
        </div>

        <div className="row">
          <button className={sharing ? "active" : ""} onClick={toggleSharing}>
            {sharing ? "🟢 Partage en direct…" : "📍 Partager ma position"}
          </button>
          {liveList.length > 0 && <span className="badge">{liveList.length} en direct</span>}
        </div>

        <div className="row alerts-row">
          {ALERT_TYPES.map((a) => (
            <button key={a.category} className="alert-btn" onClick={() => report(a.category, a.label)}>
              {a.emoji} {a.label}
            </button>
          ))}
        </div>

        <div className="row">
          <label>
            Vitesse (km/h)
            <input type="number" min={1} value={speed} onChange={(e) => setSpeed(Number(e.target.value) || 1)} />
          </label>
          <button onClick={computeRoute} disabled={positions.length < 2}>
            Itinéraire complet
          </button>
          <a className="btn" href={api.gpxUrl()} target="_blank" rel="noreferrer">
            Exporter GPX
          </a>
          <label className="btn">
            Importer GPX
            <input
              type="file"
              accept=".gpx,application/gpx+xml,text/xml"
              hidden
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) importGpx(f);
                e.target.value = "";
              }}
            />
          </label>
        </div>

        {routeInfo && <p className="info">{routeInfo}</p>}

        {stats && (
          <p className="stats">
            {stats.count} positions · longueur {stats.total_km.toFixed(1)} km
            {stats.centroid &&
              ` · centre ${stats.centroid.lat.toFixed(3)}, ${stats.centroid.lon.toFixed(3)}`}
          </p>
        )}

        <ul className="list">
          {positions.map((p) => (
            <li key={p.id}>
              <span>
                <strong>{p.label}</strong>{" "}
                <em>
                  {p.lat.toFixed(4)}, {p.lon.toFixed(4)}
                </em>
              </span>
              <span className="actions">
                <button onClick={() => rename(p)}>Renommer</button>
                <button className="danger" onClick={() => remove(p.id)}>
                  Supprimer
                </button>
              </span>
            </li>
          ))}
          {positions.length === 0 && <li className="empty">Aucune position.</li>}
        </ul>
      </section>
    </div>
  );
}

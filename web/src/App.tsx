import { useCallback, useEffect, useRef, useState } from "react";
import { api, getApiBase, setApiBase, wsUrl } from "./api";
import { MapView } from "./MapView";
import type { Alert, Coord, LiveUser, Position, Stats } from "./types";

const ALERT_TYPES = [
  { category: "police", emoji: "🚓", label: "Police" },
  { category: "accident", emoji: "💥", label: "Accident" },
  { category: "bouchon", emoji: "🚧", label: "Bouchon" },
  { category: "danger", emoji: "⚠️", label: "Danger" },
];

const LIVE_TTL = 15_000; // une voiture live disparaît après 15 s sans nouvelle
const ALERT_TTL = 30 * 60_000; // un signalement expire après 30 min

export function App() {
  const [positions, setPositions] = useState<Position[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [routeInfo, setRouteInfo] = useState("");
  const [speed, setSpeed] = useState(50);
  const [apiBase, setApiBaseState] = useState(getApiBase());
  const [error, setError] = useState("");
  const [connected, setConnected] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [liveUsers, setLiveUsers] = useState<Record<number, LiveUser>>({});
  const [alerts, setAlerts] = useState<Alert[]>([]);

  const wsRef = useRef<WebSocket | null>(null);
  const myPos = useRef<Coord | null>(null);
  const watchId = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError("");
      const [pos, st] = await Promise.all([api.positions(), api.stats()]);
      setPositions(pos);
      setStats(st);
    } catch {
      setError(`Impossible de joindre l'API (${getApiBase() || "même serveur"}).`);
    }
  }, []);

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
            [ev.id]: { id: ev.id, lat: ev.lat, lon: ev.lon, label: ev.label, ts: Date.now() },
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
  }, [apiBase, refresh]);

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

  const send = (msg: unknown) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(msg));
  };

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
    const label = prompt("Ton nom de conducteur ?", "Moi") ?? "Moi";
    watchId.current = navigator.geolocation.watchPosition(
      (p) => {
        const c = { lat: p.coords.latitude, lon: p.coords.longitude };
        myPos.current = c;
        send({ kind: "live", lat: c.lat, lon: c.lon, label });
      },
      () => setError("Partage de position refusé."),
      { enableHighAccuracy: true, maximumAge: 2000 },
    );
    setSharing(true);
  };

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

  const saveApiBase = () => {
    setApiBase(apiBase);
    setApiBaseState(getApiBase());
    refresh();
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
          <input
            value={apiBase}
            onChange={(e) => setApiBaseState(e.target.value)}
            placeholder="URL de l'API (vide = même serveur)"
          />
          <button onClick={saveApiBase}>Connecter</button>
        </div>
      </header>

      {error && <div className="error">{error}</div>}

      <MapView positions={positions} liveUsers={liveList} alerts={alerts} onAddPoint={addPoint} />

      <section className="controls">
        <p className="hint">Astuce : clique sur la carte pour ajouter une position.</p>

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

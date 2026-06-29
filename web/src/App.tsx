import { useCallback, useEffect, useState } from "react";
import { api, getApiBase, setApiBase } from "./api";
import { MapView } from "./MapView";
import type { Coord, Position, Stats } from "./types";

export function App() {
  const [positions, setPositions] = useState<Position[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [routeInfo, setRouteInfo] = useState<string>("");
  const [speed, setSpeed] = useState(50);
  const [apiBase, setApiBaseState] = useState(getApiBase());
  const [error, setError] = useState<string>("");

  const refresh = useCallback(async () => {
    try {
      setError("");
      const [pos, st] = await Promise.all([api.positions(), api.stats()]);
      setPositions(pos);
      setStats(st);
    } catch (e) {
      setError(`Impossible de joindre l'API (${getApiBase()}).`);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const addPoint = useCallback(
    async (coord: Coord) => {
      const label = prompt("Nom de la position ?", "Point") ?? "Point";
      try {
        await api.add({ lat: coord.lat, lon: coord.lon, label });
        await refresh();
      } catch {
        setError("Échec de l'ajout.");
      }
    },
    [refresh],
  );

  const remove = async (id: number) => {
    await api.remove(id);
    await refresh();
  };

  const rename = async (p: Position) => {
    const label = prompt("Nouveau nom ?", p.label);
    if (label == null) return;
    await api.update(p.id, { lat: p.lat, lon: p.lon, label });
    await refresh();
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
    const text = await file.text();
    await api.importGpx(text);
    await refresh();
  };

  const saveApiBase = () => {
    setApiBase(apiBase);
    setApiBaseState(getApiBase());
    refresh();
  };

  return (
    <div className="app">
      <header>
        <h1>🛰️ MonCap GPS</h1>
        <div className="apibar">
          <input
            value={apiBase}
            onChange={(e) => setApiBaseState(e.target.value)}
            placeholder="URL de l'API (ex. https://mon-app.herokuapp.com)"
          />
          <button onClick={saveApiBase}>Connecter</button>
        </div>
      </header>

      {error && <div className="error">{error}</div>}

      <MapView positions={positions} onAddPoint={addPoint} />

      <section className="controls">
        <p className="hint">Astuce : clique sur la carte pour ajouter une position.</p>
        <div className="row">
          <label>
            Vitesse (km/h)
            <input
              type="number"
              min={1}
              value={speed}
              onChange={(e) => setSpeed(Number(e.target.value) || 1)}
            />
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

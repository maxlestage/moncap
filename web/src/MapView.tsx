import { useEffect, useRef } from "react";
import { avatarUrl } from "./avatars";
import type { Alert, Coord, LiveUser, Position } from "./types";

interface Destination {
  lat: number;
  lon: number;
  label: string;
}

interface Props {
  positions: Position[];
  liveUsers: LiveUser[];
  alerts: Alert[];
  me: Coord | null;
  destination: Destination | null;
  onAddPoint: (coord: Coord) => void;
  onVoteAlert: (id: number, up: boolean) => void;
}

const ALERT_EMOJI: Record<string, string> = {
  police: "🚓",
  accident: "💥",
  bouchon: "🚧",
  danger: "⚠️",
  vehicule: "🚘",
  objet: "📦",
  travaux: "🏗️",
  brouillard: "🌫️",
};

/** Échappe le HTML : les popups Leaflet interprètent le HTML (anti-XSS). */
function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Carte Leaflet : positions enregistrées + voitures live + signalements. */
export function MapView({ positions, liveUsers, alerts, me, destination, onAddPoint, onVoteAlert }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<any>(null);
  const posLayer = useRef<any>(null);
  const liveLayer = useRef<any>(null);
  const alertLayer = useRef<any>(null);
  const destLayer = useRef<any>(null);
  const fittedDest = useRef<string>("");

  // Initialise la carte une seule fois.
  useEffect(() => {
    if (mapRef.current || !containerRef.current) return;
    if (typeof L === "undefined") return; // Leaflet (CDN) indisponible
    const map = L.map(containerRef.current).setView([46.6, 2.5], 5);
    L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
      attribution: "© OpenStreetMap",
      maxZoom: 19,
    }).addTo(map);
    posLayer.current = L.layerGroup().addTo(map);
    liveLayer.current = L.layerGroup().addTo(map);
    alertLayer.current = L.layerGroup().addTo(map);
    destLayer.current = L.layerGroup().addTo(map);
    map.on("click", (e: any) => onAddPoint({ lat: e.latlng.lat, lon: e.latlng.lng }));
    mapRef.current = map;
  }, [onAddPoint]);

  // Positions enregistrées + polyligne de l'itinéraire.
  useEffect(() => {
    const map = mapRef.current;
    const layer = posLayer.current;
    if (!map || !layer) return;
    layer.clearLayers();
    for (const p of positions) {
      L.marker([p.lat, p.lon]).bindPopup(esc(p.label)).addTo(layer);
    }
    if (positions.length >= 2) {
      const line = L.polyline(
        positions.map((p) => [p.lat, p.lon]),
        { color: "#2563eb", weight: 3 },
      ).addTo(layer);
      map.fitBounds(line.getBounds(), { padding: [40, 40] });
    } else if (positions.length === 1) {
      map.setView([positions[0].lat, positions[0].lon], 12);
    }
  }, [positions]);

  // Voitures live (autres utilisateurs).
  useEffect(() => {
    const layer = liveLayer.current;
    if (!layer) return;
    layer.clearLayers();
    for (const u of liveUsers) {
      const icon = L.divIcon({
        html: `<img src="${avatarUrl(u.avatar)}" class="van-marker" alt="" />`,
        className: "",
        iconSize: [44, 44],
        iconAnchor: [22, 22],
      });
      L.marker([u.lat, u.lon], { icon })
        .bindPopup(esc(u.label))
        .addTo(layer);
    }
  }, [liveUsers]);

  // Destination choisie : marqueur « drapeau » + trait depuis ma position.
  useEffect(() => {
    const map = mapRef.current;
    const layer = destLayer.current;
    if (!map || !layer) return;
    layer.clearLayers();
    if (!destination) {
      fittedDest.current = "";
      return;
    }
    const icon = L.divIcon({
      html: `<div class="dest-pin">🎯</div>`,
      className: "",
      iconSize: [30, 30],
      iconAnchor: [15, 30],
    });
    L.marker([destination.lat, destination.lon], { icon })
      .bindPopup(esc(destination.label))
      .addTo(layer);
    let line: any = null;
    if (me) {
      line = L.polyline(
        [
          [me.lat, me.lon],
          [destination.lat, destination.lon],
        ],
        { color: "#16a34a", weight: 4, dashArray: "8 8" },
      ).addTo(layer);
    }
    // On ne recadre la carte qu'une fois par destination (pas à chaque relevé GPS).
    const key = `${destination.lat},${destination.lon}`;
    if (fittedDest.current !== key) {
      fittedDest.current = key;
      if (line) {
        map.fitBounds(line.getBounds(), { padding: [50, 50], maxZoom: 13 });
      } else {
        map.setView([destination.lat, destination.lon], 12);
      }
    }
  }, [destination, me]);

  // Signalements (façon Waze) : la popup permet de voter 👍 / 👎.
  useEffect(() => {
    const layer = alertLayer.current;
    if (!layer) return;
    layer.clearLayers();
    for (const a of alerts) {
      const emoji = ALERT_EMOJI[a.category] ?? "⚠️";
      const icon = L.divIcon({
        html: `<div class="alert-pin">${emoji}</div>`,
        className: "",
        iconSize: [30, 30],
        iconAnchor: [15, 15],
      });
      const popup = document.createElement("div");
      popup.className = "alert-popup";
      const title = document.createElement("div");
      title.textContent = `${emoji} ${a.label || a.category}`;
      const counts = document.createElement("div");
      counts.className = "alert-votes";
      counts.textContent = `👍 ${a.confirms ?? 0} · 👎 ${a.denies ?? 0}`;
      const up = document.createElement("button");
      up.textContent = "👍 Toujours là";
      up.onclick = () => onVoteAlert(a.id, true);
      const down = document.createElement("button");
      down.textContent = "👎 Plus là";
      down.onclick = () => onVoteAlert(a.id, false);
      popup.append(title, counts, up, down);
      L.marker([a.lat, a.lon], { icon }).bindPopup(popup).addTo(layer);
    }
  }, [alerts, onVoteAlert]);

  return <div ref={containerRef} className="map" />;
}

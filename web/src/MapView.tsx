import { useEffect, useRef } from "react";
import type { Alert, Coord, LiveUser, Position } from "./types";

interface Props {
  positions: Position[];
  liveUsers: LiveUser[];
  alerts: Alert[];
  onAddPoint: (coord: Coord) => void;
}

const ALERT_EMOJI: Record<string, string> = {
  police: "🚓",
  accident: "💥",
  bouchon: "🚧",
  danger: "⚠️",
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
export function MapView({ positions, liveUsers, alerts, onAddPoint }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<any>(null);
  const posLayer = useRef<any>(null);
  const liveLayer = useRef<any>(null);
  const alertLayer = useRef<any>(null);

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
      L.circleMarker([u.lat, u.lon], {
        radius: 8,
        color: "#16a34a",
        fillColor: "#22c55e",
        fillOpacity: 0.9,
      })
        .bindPopup(`🚗 ${esc(u.label)}`)
        .addTo(layer);
    }
  }, [liveUsers]);

  // Signalements (façon Waze).
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
      L.marker([a.lat, a.lon], { icon })
        .bindPopup(`${emoji} ${esc(a.label || a.category)}`)
        .addTo(layer);
    }
  }, [alerts]);

  return <div ref={containerRef} className="map" />;
}

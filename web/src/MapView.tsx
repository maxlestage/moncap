import { useEffect, useRef } from "react";
import type { Coord, Position } from "./types";

interface Props {
  positions: Position[];
  onAddPoint: (coord: Coord) => void;
}

/** Carte Leaflet : marqueurs des positions + polyligne de l'itinéraire. */
export function MapView({ positions, onAddPoint }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<any>(null);
  const layerRef = useRef<any>(null);

  // Initialise la carte une seule fois.
  useEffect(() => {
    if (mapRef.current || !containerRef.current) return;
    if (typeof L === "undefined") return; // Leaflet (CDN) indisponible
    const map = L.map(containerRef.current).setView([46.6, 2.5], 5);
    L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
      attribution: "© OpenStreetMap",
      maxZoom: 19,
    }).addTo(map);
    layerRef.current = L.layerGroup().addTo(map);
    map.on("click", (e: any) => onAddPoint({ lat: e.latlng.lat, lon: e.latlng.lng }));
    mapRef.current = map;
  }, [onAddPoint]);

  // Redessine marqueurs + polyligne à chaque changement de positions.
  useEffect(() => {
    const map = mapRef.current;
    const layer = layerRef.current;
    if (!map || !layer) return; // carte non initialisée (Leaflet absent)
    layer.clearLayers();

    for (const p of positions) {
      L.marker([p.lat, p.lon]).bindPopup(p.label).addTo(layer);
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

  return <div ref={containerRef} className="map" />;
}

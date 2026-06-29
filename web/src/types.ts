export interface Position {
  id: number;
  lat: number;
  lon: number;
  label: string;
}

export interface NewPosition {
  lat: number;
  lon: number;
  label: string;
}

export interface Coord {
  lat: number;
  lon: number;
}

export interface MultiRouteResponse {
  total_km: number;
  legs_km: number[];
  duration_min: number;
}

export interface BBox {
  min_lat: number;
  min_lon: number;
  max_lat: number;
  max_lon: number;
}

export interface Stats {
  count: number;
  total_km: number;
  bbox: BBox | null;
  centroid: Coord | null;
}

/** Position GPS en direct d'un autre utilisateur. */
export interface LiveUser {
  id: number;
  lat: number;
  lon: number;
  label: string;
  ts: number;
}

/** Signalement façon Waze. */
export interface Alert {
  id: number;
  category: string;
  lat: number;
  lon: number;
  label: string;
  ts: number;
}

// Leaflet est chargé via CDN dans index.html ; on l'utilise via le global L.
declare const L: any;

// Import d'images (bundlées par Bun, renvoie une URL).
declare module "*.png" {
  const url: string;
  export default url;
}

import { createRoot } from "react-dom/client";
import { App } from "./App";
import { LANGUES } from "./langues";
import "./styles.css";

/**
 * Mode développement seulement : la construction rend les pages sans
 * JavaScript. On y affiche le français, avec le logo pris à la source.
 */
const racine = document.getElementById("root");
if (!racine) throw new Error("Élément #root introuvable");
const fr = LANGUES[0];
if (!fr) throw new Error("Aucune langue définie");
createRoot(racine).render(<App c={fr} res={{ logo: "/assets/logo.webp" }} />);

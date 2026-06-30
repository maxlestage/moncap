import blue from "./avatars/blue.png";
import green from "./avatars/green.png";
import mint from "./avatars/mint.png";
import orange from "./avatars/orange.png";
import alpine from "./avatars/alpine.png";
import ferrari from "./avatars/ferrari.png";
import merc1 from "./avatars/merc1.png";
import merc2 from "./avatars/merc2.png";
import red1 from "./avatars/red1.png";
import yarisGrey from "./avatars/yarisGrey.png";
import yarisWhite from "./avatars/yarisWhite.png";
import yarisBlue from "./avatars/yarisBlue.png";
import abarthWhite from "./avatars/abarthWhite.png";
import abarthRed from "./avatars/abarthRed.png";

export interface Avatar {
  id: string;
  url: string;
  label: string;
}

/** Avatars sélectionnables : combis Volkswagen + voitures de F1. */
export const AVATARS: Avatar[] = [
  { id: "green", url: green, label: "Combi vert" },
  { id: "orange", url: orange, label: "Combi orange" },
  { id: "blue", url: blue, label: "Combi bleu" },
  { id: "mint", url: mint, label: "Combi menthe" },
  { id: "ferrari", url: ferrari, label: "Ferrari" },
  { id: "alpine", url: alpine, label: "Alpine" },
  { id: "merc1", url: merc1, label: "Mercedes argent" },
  { id: "merc2", url: merc2, label: "Mercedes noire" },
  { id: "red1", url: red1, label: "F1 rouge" },
  { id: "yarisGrey", url: yarisGrey, label: "GR Yaris gris" },
  { id: "yarisWhite", url: yarisWhite, label: "GR Yaris blanc" },
  { id: "yarisBlue", url: yarisBlue, label: "GR Yaris bleu" },
  { id: "abarthWhite", url: abarthWhite, label: "Abarth 500 blanc" },
  { id: "abarthRed", url: abarthRed, label: "Abarth 595 rouge" },
];

export function avatarUrl(id: string): string {
  return AVATARS.find((a) => a.id === id)?.url ?? AVATARS[0].url;
}

const KEY = "moncap.avatar";

export function getAvatar(): string {
  return localStorage.getItem(KEY) ?? "green";
}

export function setAvatar(id: string): void {
  localStorage.setItem(KEY, id);
}

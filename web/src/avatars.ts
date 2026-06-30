import blue from "./avatars/blue.png";
import green from "./avatars/green.png";
import mint from "./avatars/mint.png";
import orange from "./avatars/orange.png";

export interface Avatar {
  id: string;
  url: string;
  label: string;
}

/** Avatars « combi » sélectionnables. */
export const AVATARS: Avatar[] = [
  { id: "green", url: green, label: "Combi vert" },
  { id: "orange", url: orange, label: "Combi orange" },
  { id: "blue", url: blue, label: "Combi bleu" },
  { id: "mint", url: mint, label: "Combi menthe" },
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

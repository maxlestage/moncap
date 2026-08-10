import { APP } from "./appstrings";
import type { LangueApp } from "./appstrings";

/**
 * Les libellés de l'application, dans la langue de la page.
 *
 * `APP` est engendré depuis le même tableau que le catalogue iOS : ce que la
 * maquette affiche est exactement ce que l'application affiche. Une clé
 * absente retombe sur elle-même — c'est aussi ce que fait SwiftUI.
 */
export function app(langue: LangueApp, cle: string): string {
  return APP[langue][cle] ?? cle;
}

/**
 * Remplit un format à la façon de `String(format:)`.
 *
 * Volontairement minimal : seuls les spécificateurs employés par
 * l'application (`%.0f`, `%.1f`, `%.2f`, `%d`, `%@`) sont reconnus, dans
 * l'ordre. Il ne s'agit pas de réimplémenter printf mais de verser quatre
 * nombres dans une phrase déjà traduite.
 *
 * Les décimales gardent le **point**, parce que `String(format:)` sans locale
 * fait de même : l'application écrit « 24.3 km » y compris en français, et
 * une maquette qui écrirait « 24,3 » mentirait sur ce détail-là.
 */
export function format(modele: string, valeurs: readonly (number | string)[]): string {
  let i = 0;
  return modele.replace(/%(?:\.(\d)f|d|@)/g, (spec) => {
    const v = valeurs[i++];
    if (v === undefined) return spec;
    if (typeof v === "string") return v;
    const decimales = /^%\.(\d)f$/.exec(spec);
    return decimales ? v.toFixed(Number(decimales[1])) : String(Math.round(v));
  });
}

/** Applique `format` à un libellé traduit de l'application. */
export function appFormat(
  langue: LangueApp,
  cle: string,
  valeurs: readonly (number | string)[],
): string {
  return format(app(langue, cle), valeurs);
}

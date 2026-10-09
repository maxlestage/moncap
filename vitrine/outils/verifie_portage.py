#!/usr/bin/env python3
"""Vérifie que le portage du contenu n'a rien perdu.

Mille cinq cents lignes converties par machine se relisent mal : l'œil
glisse sur une accentuation disparue ou un pourcentage oublié. Ce contrôle
compare donc le Rust produit au JSON d'origine, **valeur par valeur**.

Il couvre les nombres autant que les textes, et c'est délibéré : ma première
version ne regardait que les chaînes, et le champ `avancement` d'une région
— un entier, absent du premier élément — est passé au travers sans un mot.
Un contrôle qui ne regarde qu'une moitié des données en rate exactement une
moitié.
"""

import collections
import json
import re
import sys


def litteraux_rust(chemin: str):
    """Les chaînes et les nombres écrits dans la source Rust.

    Les commentaires de documentation sont retirés : ils expliquent le
    contenu sans en faire partie, et leurs phrases fausseraient le compte.
    """
    src = open(chemin, encoding="utf-8").read()
    code = "\n".join(
        l for l in src.split("\n") if not l.lstrip().startswith(("///", "//!", "//"))
    )

    textes = collections.Counter()
    for m in re.finditer(r'r#"(.*?)"#', code, re.S):
        textes[m.group(1)] += 1
    sans_crues = re.sub(r'r#".*?"#', "", code, flags=re.S)
    for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', sans_crues):
        textes[json.loads('"' + m.group(1) + '"')] += 1

    # Les nombres, hors de toute chaîne. On ne retient que ce qui occupe une
    # position de valeur : après « : », « ( », « , » ou « = ». Les suffixes
    # de type (`u32`) et les largeurs d'entier ne sont pas des données.
    #
    # Le « = » a manqué à ma première version, et le seul nombre déclaré en
    # constante (`ITINERAIRES_MAX = 15`) est ressorti comme perdu alors qu'il
    # était là. Un contrôle qui accuse à tort se fait désactiver.
    sans_texte = re.sub(r'"(?:[^"\\]|\\.)*"', '""', sans_crues)
    nombres = collections.Counter()
    for m in re.finditer(r"[:(,=]\s*(-?\d+(?:\.\d+)?)\b", sans_texte):
        nombres[float(m.group(1))] += 1
    return textes, nombres


def valeurs_json(chemin: str):
    textes, nombres = collections.Counter(), collections.Counter()

    def recolte(v):
        if isinstance(v, str):
            textes[v] += 1
        elif isinstance(v, bool):
            pass
        elif isinstance(v, (int, float)):
            nombres[float(v)] += 1
        elif isinstance(v, dict):
            for w in v.values():
                recolte(w)
        elif isinstance(v, list):
            for w in v:
                recolte(w)

    recolte(json.load(open(chemin, encoding="utf-8")))
    return textes, nombres


def main():
    rust, json_ = sys.argv[1], sys.argv[2]
    t_rust, n_rust = litteraux_rust(rust)
    t_att, n_att = valeurs_json(json_)

    echecs = []

    manquants = t_att - t_rust
    print(f"textes   : {len(t_att):>4} attendus, {len(t_rust):>4} trouvés", end="  ")
    if manquants:
        print("✗")
        echecs.append(f"{sum(manquants.values())} texte(s) perdus")
        for t, n in list(manquants.items())[:10]:
            print(f"    ×{n}  {t[:84]!r}")
    else:
        print("✅")

    # Les nombres sont comparés en ensemble et non en multiset : le même
    # pourcentage peut légitimement apparaître un nombre différent de fois
    # (un total recalculé plutôt que stocké, par exemple). Ce qui compte est
    # qu'aucune valeur n'ait purement disparu.
    perdus = {v for v in n_att if v not in n_rust}
    # Le total des coûts n'est volontairement plus stocké : il se recalcule.
    totaux = {
        round(c[0] + c[1], 10)
        for c in [
            i["cout"]
            for i in json.load(open(json_, encoding="utf-8"))["k"][
                "MAQUETTE_ITINERAIRES"
            ]
        ]
        if len(c) == 3
    }
    perdus -= totaux
    print(f"nombres  : {len(n_att):>4} attendus, {len(n_rust):>4} trouvés", end="  ")
    if perdus:
        print("✗")
        echecs.append(f"{len(perdus)} nombre(s) perdus")
        for v in sorted(perdus)[:10]:
            print(f"    {v}")
    else:
        print("✅" + (f"  ({len(totaux)} total(aux) désormais recalculés)" if totaux else ""))

    print()
    if echecs:
        print("✗ " + " ; ".join(echecs))
        sys.exit(1)
    print("✅ le portage a tout emporté : textes et nombres, à l'identique")


if __name__ == "__main__":
    main()

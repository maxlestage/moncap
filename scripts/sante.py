#!/usr/bin/env python3
"""Vérifie que la production rend vraiment ses services, et pas seulement
qu'elle répond.

Pourquoi ce fichier existe : `/health` répondait 200 pendant que
`/offline/regions` rendait 502 — le jeton GitHub du serveur avait expiré, et
les paquets hors ligne n'étaient plus servis du tout. Personne ne l'a vu
pendant des semaines, parce que le seul contrôle en place demandait au
serveur s'il était debout, pas s'il savait faire son métier.

Un service qui répond n'est pas un service qui marche. Chaque contrôle ici
demande donc une **vraie** réponse : une liste non vide, une redirection vers
un fichier, une page qui contient sa phrase.

N'exige aucun secret : tout ce qui est éprouvé ici est public, comme
l'application le voit. C'est ce qui permet de le lancer à tout moment, y
compris quand on n'a justement plus les clés.
"""

import os
import sys

import requests

BASE = os.environ.get("BASE", "https://moncap-c41a5aaf07e8.herokuapp.com")
# Une région connue pour éprouver le courtier de paquets. Sa seule qualité
# requise : exister.
REGION = os.environ.get("REGION", "tuiles-eu-france-aquitaine")

echecs: list[str] = []
lignes: list[str] = []


def dit(ok, quoi, detail=""):
    lignes.append(f"  {'✓' if ok else '✗'} {quoi}{' — ' + detail if detail else ''}")
    if not ok:
        echecs.append(f"{quoi}{' : ' + detail if detail else ''}")


def get(chemin, **kw):
    try:
        return requests.get(f"{BASE}{chemin}", timeout=60, **kw)
    except requests.RequestException as e:
        return e


def sante():
    r = get("/health")
    if isinstance(r, Exception):
        return dit(False, "le serveur répond", str(r))
    dit(r.status_code == 200, "le serveur répond", f"HTTP {r.status_code}")


def pages():
    """Les trois langues de la vitrine, avec leur phrase propre."""
    for chemin, phrase in (
        ("/", "Comparez, puis roulez"),
        ("/es", "Compara y conduce"),
        ("/en", "Compare, then drive"),
    ):
        r = get(chemin)
        if isinstance(r, Exception):
            dit(False, f"page {chemin}", str(r))
            continue
        bon = r.status_code == 200 and phrase in r.text
        dit(bon, f"page {chemin}", "" if bon else f"HTTP {r.status_code}, phrase absente")


def paquets():
    """Le courtier de paquets hors ligne — le contrôle qui manquait.

    Deux questions distinctes : sait-il **lister** les régions, et sait-il en
    **servir** une. La première a échoué en silence pendant des semaines ; la
    seconde échouerait pour les mêmes raisons mais concerne l'utilisateur qui
    télécharge, pas celui qui parcourt la liste.
    """
    r = get("/offline/regions")
    if isinstance(r, Exception):
        dit(False, "liste des régions hors ligne", str(r))
    elif r.status_code != 200:
        # Le serveur explique lui-même ses pannes : on reprend son message
        # plutôt que d'en inventer un.
        dit(False, "liste des régions hors ligne",
            f"HTTP {r.status_code} — {r.text.strip()[:160]}")
    else:
        try:
            regions = r.json()
        except ValueError:
            regions = None
        if isinstance(regions, list) and regions:
            dit(True, "liste des régions hors ligne", f"{len(regions)} régions")
        else:
            # Une liste vide est le symptôme le plus traître : HTTP 200, et
            # une app qui n'affiche rien sans rien pouvoir expliquer.
            dit(False, "liste des régions hors ligne",
                "réponse 200 mais liste vide — rien à proposer au téléchargement")

    # Servir un fichier : la redirection signée est la vraie preuve.
    for nature in ("archive", "rues", "lieux", "vitesses"):
        r = get(f"/offline/regions/{REGION}/{nature}", allow_redirects=False)
        if isinstance(r, Exception):
            dit(False, f"paquet {nature}", str(r))
            continue
        # 302 vers l'adresse signée de GitHub : le courtier a fait son travail.
        bon = r.status_code in (301, 302, 307)
        dit(bon, f"paquet {nature} ({REGION})",
            "" if bon else f"HTTP {r.status_code} — {r.text.strip()[:120]}")


def main():
    print(f"Santé de {BASE}\n")
    sante()
    pages()
    paquets()
    print("\n".join(lignes))
    if echecs:
        print(f"\n{len(echecs)} service(s) en panne :")
        for e in echecs:
            print(f"  - {e}")
        print("::error::la production ne rend pas tous ses services")
        sys.exit(1)
    print("\nTous les services répondent et rendent ce qu'on leur demande.")


if __name__ == "__main__":
    main()

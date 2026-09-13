#!/usr/bin/env python3
"""Pose sur Heroku les variables du serveur qui ne peuvent pas vivre dans
le dépôt : la liste d'invités (PLUS_OFFERT) et le compte d'envoi de
courriel (SMTP_*).

Pourquoi pas à la main : `PLUS_OFFERT` est une liste, et une liste se
modifie mal à la souris — coller par-dessus efface les invités déjà
présents. Ici on **ajoute** sans jamais retirer, et relancer ne double
personne.

Le compte de review d'Apple y a sa place obligée : son essai de quatorze
jours court depuis la création du compte, et une review qui arrive au
quinzième jour tomberait sur l'app verrouillée. Sur la liste, il a tout,
pour toujours.

Deux modes, par la variable MODE :
  - `verifie` (défaut) : ne touche à rien, dit ce qui manque.
  - `applique`         : pose ce qui manque, et seulement cela.

Les secrets SMTP_* sont facultatifs : absents, le serveur sait déjà s'en
passer (`smtp_configure()` dans src/main.rs) et le script les ignore sans
se plaindre.
"""

import json
import os
import sys

import requests

API = "https://api.heroku.com"
APP = os.environ.get("HEROKU_APP", "moncap-c41a5aaf07e8")
MODE = os.environ.get("MODE", "verifie")

# Le compte que la review d'Apple utilise (voir frontend/APPSTORE.md).
INVITES = ["review.apple@moncap-gps.fr"]

# Reprises des secrets GitHub du même nom quand ils existent.
SMTP = ["SMTP_HOST", "SMTP_USER", "SMTP_PASS", "SMTP_FROM", "SMTP_PORT"]


def die(msg):
    print(f"::error::{msg}")
    sys.exit(1)


def entetes():
    cle = os.environ.get("HEROKU_API_KEY", "")
    if not cle:
        die(
            "HEROKU_API_KEY absent. Crée le secret dans le dépôt "
            "(Settings → Secrets → Actions) avec la sortie de "
            "`heroku authorizations:create`, puis relance."
        )
    return {
        "Authorization": f"Bearer {cle}",
        "Accept": "application/vnd.heroku+json; version=3",
        "Content-Type": "application/json",
    }


def config_actuelle():
    r = requests.get(f"{API}/apps/{APP}/config-vars", headers=entetes(), timeout=60)
    if r.status_code == 401:
        die("Heroku refuse la clé (401) : jeton expiré ou révoqué.")
    if r.status_code == 404:
        die(f"App Heroku « {APP} » introuvable pour ce compte.")
    if r.status_code >= 300:
        die(f"GET config-vars -> {r.status_code}: {r.text[:400]}")
    return r.json()


def applique(changements):
    r = requests.patch(
        f"{API}/apps/{APP}/config-vars",
        headers=entetes(),
        data=json.dumps(changements),
        timeout=60,
    )
    if r.status_code >= 300:
        die(f"PATCH config-vars -> {r.status_code}: {r.text[:400]}")


def liste_invites(actuel):
    """Ajoute les invités manquants sans toucher aux autres.

    Le serveur lit cette variable en séparant sur les virgules et en
    coupant les espaces (`est_invite` dans src/main.rs) ; la comparaison
    se fait donc sur la valeur nettoyée, pas sur la chaîne brute.
    """
    brut = actuel.get("PLUS_OFFERT") or ""
    presents = [x.strip() for x in brut.split(",") if x.strip()]
    ajouts = [c for c in INVITES if c not in presents]
    if not ajouts:
        return None, f"PLUS_OFFERT : déjà {len(presents)} invité(s), rien à ajouter"
    return ",".join(presents + ajouts), (
        f"PLUS_OFFERT : ajouter {', '.join(ajouts)} "
        f"(la liste passe de {len(presents)} à {len(presents) + len(ajouts)})"
    )


def main():
    if MODE not in ("verifie", "applique"):
        die(f"MODE={MODE} inconnu (attendu : verifie ou applique)")
    actuel = config_actuelle()
    print(f"App Heroku {APP} · mode {MODE}\n")

    changements = {}
    notes = []

    valeur, note = liste_invites(actuel)
    notes.append(note)
    if valeur is not None:
        changements["PLUS_OFFERT"] = valeur

    for nom in SMTP:
        fourni = os.environ.get(nom, "").strip()
        if not fourni:
            etat = "présent sur Heroku" if actuel.get(nom) else "absent des deux côtés"
            notes.append(f"{nom} : pas de secret fourni ({etat})")
            continue
        if actuel.get(nom) == fourni:
            notes.append(f"{nom} : déjà à jour")
            continue
        changements[nom] = fourni
        notes.append(f"{nom} : à poser depuis le secret du dépôt")

    for ligne in notes:
        print(f"  · {ligne}")

    if not changements:
        print("\nRien à faire : la configuration est déjà celle attendue.")
        return

    if MODE != "applique":
        print(
            f"\n{len(changements)} variable(s) à poser. "
            "Relance en mode « applique » pour le faire."
        )
        return

    applique(changements)
    # Les valeurs ne sont jamais réimprimées : un mot de passe SMTP dans un
    # journal d'action publique y reste pour toujours.
    print(f"\n✓ {len(changements)} variable(s) posée(s) : {', '.join(sorted(changements))}")
    print("Heroku redémarre le dyno tout seul ; /health revient en quelques secondes.")


if __name__ == "__main__":
    main()

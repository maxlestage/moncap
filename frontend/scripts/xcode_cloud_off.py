#!/usr/bin/env python3
"""Désactive tous les workflows Xcode Cloud du compte, via l'API App Store
Connect.

Pourquoi : un pipeline Xcode Cloud « à chaque poussée de code » tournait en
parallèle de la CI GitHub, avec son propre compteur de builds (119, 120…).
Ses envois TestFlight masquaient les nôtres — l'utilisateur voyait
« build 119 » quand le run GitHub s'appelait 275 — et chaque merge payait
deux constructions. L'app est livrée par cette CI, et par elle seule.

Désactivation, pas suppression : réactivable d'un clic dans App Store
Connect si un jour Xcode Cloud redevient utile.

Dépendances : requests, pyjwt[crypto] (installées par le workflow).
"""

import json
import os
import sys
import time

import jwt
import requests

API = "https://api.appstoreconnect.apple.com/v1"


def die(msg):
    print(f"::error::{msg}")
    sys.exit(1)


def p8_content():
    raw = os.environ["ASC_KEY_P8"]
    if "BEGIN PRIVATE KEY" in raw:
        return raw
    import base64

    return base64.b64decode(raw).decode()


def make_token():
    now = int(time.time())
    payload = {"iss": os.environ["ASC_ISSUER_ID"], "iat": now, "exp": now + 1200,
               "aud": "appstoreconnect-v1"}
    headers = {"kid": os.environ["ASC_KEY_ID"], "typ": "JWT"}
    return jwt.encode(payload, p8_content(), algorithm="ES256", headers=headers)


def req(method, path, token, **kw):
    r = requests.request(method, f"{API}{path}", timeout=60,
                         headers={"Authorization": f"Bearer {token}",
                                  "Content-Type": "application/json"}, **kw)
    if r.status_code >= 300:
        die(f"API {method} {path} -> {r.status_code}: {r.text}")
    return r.json() if r.text else {}


# Seule cette app est livrée par la CI GitHub : elle seule doit perdre son
# pipeline Xcode Cloud. La première exécution coupait TOUT le compte — les
# huit autres apps comprises — d'où le mode « repare » qui les rallume.
CIBLE = "MonCapGPS"


def regle(token, wf, produit, active):
    body = {"data": {"type": "ciWorkflows", "id": wf["id"],
                     "attributes": {"isEnabled": active}}}
    req("PATCH", f"/ciWorkflows/{wf['id']}", token, data=json.dumps(body))
    verbe = "rallumé" if active else "désactivé"
    print(f"Workflow Xcode Cloud {verbe} : {produit} / {wf['attributes'].get('name')}")


def main():
    # MODE=coupe (défaut) : coupe les workflows du produit CIBLE seulement.
    # MODE=repare : rallume tous les AUTRES produits, CIBLE reste coupée.
    mode = os.environ.get("MODE", "coupe")
    token = make_token()
    produits = req("GET", "/ciProducts?limit=200", token).get("data", [])
    if not produits:
        print("Aucun produit Xcode Cloud sur ce compte — rien à faire.")
        return
    touches = 0
    for produit in produits:
        nom_produit = produit["attributes"].get("name")
        vise = (nom_produit == CIBLE) if mode == "coupe" else (nom_produit != CIBLE)
        if not vise:
            continue
        flux = req(
            "GET", f"/ciProducts/{produit['id']}/workflows?limit=200", token
        ).get("data", [])
        for wf in flux:
            actif = wf["attributes"].get("isEnabled", True)
            nom_wf = wf["attributes"].get("name")
            voulu = mode == "repare"
            # La réparation ne rallume que ce que la première exécution a
            # éteint : les workflows « Default ». « Optyx / Test » était
            # déjà coupé AVANT — un choix de l'utilisateur, pas le nôtre.
            if mode == "repare" and nom_wf != "Default":
                print(f"Laissé tel quel : {nom_produit} / {nom_wf}")
                continue
            if actif == voulu:
                print(f"Déjà bon : {nom_produit} / {nom_wf}")
                continue
            regle(token, wf, nom_produit, voulu)
            touches += 1
    print(f"{touches} workflow(s) modifié(s) (mode {mode}).")


if __name__ == "__main__":
    main()

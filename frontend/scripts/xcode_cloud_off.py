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


def main():
    token = make_token()
    produits = req("GET", "/ciProducts?limit=200", token).get("data", [])
    if not produits:
        print("Aucun produit Xcode Cloud sur ce compte — rien à couper.")
        return
    coupes = 0
    for produit in produits:
        nom_produit = produit["attributes"].get("name")
        flux = req(
            "GET", f"/ciProducts/{produit['id']}/workflows?limit=200", token
        ).get("data", [])
        for wf in flux:
            attrs = wf["attributes"]
            nom = attrs.get("name")
            if not attrs.get("isEnabled", True):
                print(f"Déjà coupé : {nom_produit} / {nom}")
                continue
            body = {"data": {"type": "ciWorkflows", "id": wf["id"],
                             "attributes": {"isEnabled": False}}}
            req("PATCH", f"/ciWorkflows/{wf['id']}", token, data=json.dumps(body))
            print(f"Workflow Xcode Cloud désactivé : {nom_produit} / {nom}")
            coupes += 1
    print(f"{coupes} workflow(s) coupé(s). TestFlight ne reçoit plus que la CI GitHub.")


if __name__ == "__main__":
    main()

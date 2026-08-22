#!/usr/bin/env python3
"""Liste les derniers builds TestFlight de l'app, avec leur état de
traitement — la réponse d'Apple, pas une supposition.

Écrit pour trancher « le build 275 est-il arrivé ? » : l'envoi a réussi
côté CI, mais TestFlight ne montre que ce qu'Apple a fini de traiter, et
un traitement échoué disparaît sans autre trace qu'un courriel.

Dépendances : requests, pyjwt[crypto] (installées par le workflow).
"""

import os
import sys
import time

import jwt
import requests

API = "https://api.appstoreconnect.apple.com/v1"
BUNDLE_ID = os.environ.get("BUNDLE_ID", "com.maxlestage.moncap")


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


def req(path, token):
    r = requests.get(f"{API}{path}", timeout=60,
                     headers={"Authorization": f"Bearer {token}"})
    if r.status_code >= 300:
        die(f"API GET {path} -> {r.status_code}: {r.text}")
    return r.json()


def main():
    token = make_token()
    apps = req(f"/apps?filter[bundleId]={BUNDLE_ID}", token).get("data", [])
    if not apps:
        die(f"Aucune app pour {BUNDLE_ID}.")
    app_id = apps[0]["id"]
    builds = req(
        f"/builds?filter[app]={app_id}&sort=-uploadedDate&limit=20"
        "&fields[builds]=version,processingState,uploadedDate,expired",
        token,
    ).get("data", [])
    print(f"{len(builds)} derniers builds (du plus récent au plus ancien) :")
    for b in builds:
        a = b["attributes"]
        etat = a.get("processingState")
        expire = " · EXPIRÉ" if a.get("expired") else ""
        print(f"  build {a.get('version'):>6} · {etat}{expire} · envoyé {a.get('uploadedDate')}")


if __name__ == "__main__":
    main()

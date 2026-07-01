#!/usr/bin/env python3
"""Génère en ligne, via l'API App Store Connect, tout ce qu'il faut pour
signer et envoyer l'app — sans Mac ni certificat fourni à la main :

  1. une paire de clés + un certificat de distribution iOS (créé via l'API) ;
  2. un fichier .p12 (clé + certificat) protégé par un mot de passe aléatoire ;
  3. un profil de provisioning App Store (créé via l'API).

Pour rester sous la limite Apple de certificats, on révoque d'abord les
certificats de distribution existants (l'app est gérée uniquement par cette
CI). Écrit les fichiers dans $RUNNER_TEMP et exporte les variables utiles
dans $GITHUB_ENV.

Dépendances : requests, pyjwt[crypto] (installées par le workflow).
"""

import base64
import json
import os
import subprocess
import sys
import time

import jwt
import requests

API = "https://api.appstoreconnect.apple.com/v1"
BUNDLE_ID = os.environ.get("BUNDLE_ID", "com.maxlestage.moncap")
PROFILE_NAME = "MonCap CI AppStore"
RUNNER_TEMP = os.environ["RUNNER_TEMP"]


def die(msg):
    print(f"::error::{msg}")
    sys.exit(1)


def p8_content():
    raw = os.environ["ASC_KEY_P8"]
    if "BEGIN PRIVATE KEY" in raw:
        return raw
    return base64.b64decode(raw).decode()


def make_token():
    now = int(time.time())
    payload = {"iss": os.environ["ASC_ISSUER_ID"], "iat": now, "exp": now + 1200,
               "aud": "appstoreconnect-v1"}
    headers = {"kid": os.environ["ASC_KEY_ID"], "typ": "JWT"}
    return jwt.encode(payload, p8_content(), algorithm="ES256", headers=headers)


def req(method, path, token, **kw):
    url = path if path.startswith("http") else f"{API}{path}"
    r = requests.request(method, url, timeout=60,
                         headers={"Authorization": f"Bearer {token}",
                                  "Content-Type": "application/json"}, **kw)
    if r.status_code >= 300:
        die(f"API {method} {path} -> {r.status_code}: {r.text}")
    return r.json() if r.text else {}


def sh(*args, **kw):
    subprocess.run(args, check=True, **kw)


def main():
    token = make_token()

    # 1) Révoque les certificats de distribution existants (leurs clés privées
    #    sont perdues de toute façon), pour rester sous la limite Apple.
    certs = req("GET", "/certificates?filter[certificateType]=IOS_DISTRIBUTION&limit=200", token)
    for c in certs.get("data", []):
        req("DELETE", f"/certificates/{c['id']}", token)
        print(f"Certificat révoqué : {c['id']}")

    # 2) Clé privée + CSR
    key_path = f"{RUNNER_TEMP}/dist_key.pem"
    csr_path = f"{RUNNER_TEMP}/dist.csr"
    sh("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
       "-keyout", key_path, "-out", csr_path,
       "-subj", "/CN=MonCap CI/O=maxlestage/C=US")
    csr = open(csr_path).read()

    # 3) Crée le certificat via l'API
    body = {"data": {"type": "certificates",
                     "attributes": {"certificateType": "IOS_DISTRIBUTION",
                                    "csrContent": csr}}}
    cert = req("POST", "/certificates", token, data=json.dumps(body))
    cert_id = cert["data"]["id"]
    cert_der = base64.b64decode(cert["data"]["attributes"]["certificateContent"])
    der_path = f"{RUNNER_TEMP}/dist.der"
    pem_path = f"{RUNNER_TEMP}/dist.pem"
    open(der_path, "wb").write(cert_der)
    sh("openssl", "x509", "-inform", "DER", "-in", der_path, "-out", pem_path)
    print(f"Certificat créé : {cert_id}")

    # 4) .p12 (clé + certificat) avec mot de passe aléatoire
    password = base64.urlsafe_b64encode(os.urandom(18)).decode()
    p12_path = f"{RUNNER_TEMP}/dist.p12"
    sh("openssl", "pkcs12", "-export",
       "-inkey", key_path, "-in", pem_path,
       "-out", p12_path, "-passout", f"pass:{password}")

    # 5) Identifiant de la fiche app (bundleId)
    bundles = req("GET", f"/bundleIds?filter[identifier]={BUNDLE_ID}&limit=1", token)
    if not bundles.get("data"):
        die(f"Aucun bundle id « {BUNDLE_ID} » sur le compte : crée d'abord la fiche app.")
    bundle_uid = bundles["data"][0]["id"]

    # 6) Profil App Store : supprime l'ancien de même nom, puis recrée
    profs = req("GET", "/profiles?filter[profileType]=IOS_APP_STORE&limit=200", token)
    for p in profs.get("data", []):
        if p["attributes"]["name"] == PROFILE_NAME:
            req("DELETE", f"/profiles/{p['id']}", token)
    body = {"data": {"type": "profiles",
                     "attributes": {"name": PROFILE_NAME, "profileType": "IOS_APP_STORE"},
                     "relationships": {
                         "bundleId": {"data": {"type": "bundleIds", "id": bundle_uid}},
                         "certificates": {"data": [{"type": "certificates", "id": cert_id}]}}}}
    prof = req("POST", "/profiles", token, data=json.dumps(body))
    pp = base64.b64decode(prof["data"]["attributes"]["profileContent"])
    pp_path = f"{RUNNER_TEMP}/pp.mobileprovision"
    open(pp_path, "wb").write(pp)
    print(f"Profil créé : {PROFILE_NAME}")

    with open(os.environ["GITHUB_ENV"], "a") as f:
        f.write(f"P12_PATH={p12_path}\n")
        f.write(f"P12_PASSWORD={password}\n")
        f.write(f"PP_PATH={pp_path}\n")
        f.write(f"PROFILE_NAME={PROFILE_NAME}\n")
    print("::add-mask::" + password)


if __name__ == "__main__":
    main()

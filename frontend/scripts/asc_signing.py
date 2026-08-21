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

    # 0) Diagnostic : liste les apps que cette clé API voit réellement, avec
    #    leur bundle ID. Permet de repérer un décalage de compte / bundle ID.
    apps = req("GET", "/apps?limit=200&fields[apps]=bundleId,name", token)
    data = apps.get("data", [])
    print(f"Apps visibles par cette clé API ({len(data)}) :")
    found = False
    for a in data:
        bid = a["attributes"].get("bundleId")
        print(f"  - {bid}  ({a['attributes'].get('name')})")
        if bid == BUNDLE_ID:
            found = True
    if not found:
        print(f"::warning::La fiche « {BUNDLE_ID} » n'est PAS visible par cette clé "
              f"API. Crée l'app avec ce bundle ID sur le compte de la clé, ou "
              f"vérifie que la clé appartient au bon compte.")

    # 1) Révoque les certificats que cette CI laisse derrière elle, pour
    #    rester sous la limite Apple :
    #    - la distribution, en entier — ses clés privées sont perdues à
    #      chaque exécuteur de toute façon, l'app n'est signée que par ici ;
    #    - le développement, mais SEULEMENT les « Created via API » : chaque
    #      exécuteur est une machine neuve, xcodebuild y fabrique un
    #      certificat de développement à chaque archive
    #      (-allowProvisioningUpdates) sans jamais le nettoyer, et le quota
    #      finit atteint — « Choose a certificate to revoke », constaté au
    #      run 253. Les certificats d'Xcode Cloud ou d'un Mac personnel
    #      portent d'autres noms : on n'y touche pas.
    for ctype in ("IOS_DISTRIBUTION", "DEVELOPMENT", "IOS_DEVELOPMENT"):
        certs = req(
            "GET", f"/certificates?filter[certificateType]={ctype}&limit=200", token
        )
        for c in certs.get("data", []):
            nom = c["attributes"].get("displayName") or ""
            if ctype != "IOS_DISTRIBUTION" and "Created via API" not in nom:
                continue
            req("DELETE", f"/certificates/{c['id']}", token)
            print(f"Certificat révoqué ({ctype}) : {c['id']} « {nom} »")

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

    # 5) Bundle IDs (App IDs) : on les récupère, ou on les enregistre via
    #    l'API. L'app ET l'extension Guidage (Live Activity) : xcodebuild
    #    sait créer des profils (-allowProvisioningUpdates) mais pas
    #    enregistrer un identifiant nouveau — « Automatic signing cannot
    #    register bundle identifier », constaté au premier export à deux
    #    cibles. L'API, elle, le peut ; une fois enregistré, c'est acquis
    #    pour toujours, Xcode Cloud compris.
    def assure_bundle(identifier, nom):
        # limit large et comparaison exacte : le filtre de l'API matche par
        # préfixe, « com.x » renvoie aussi « com.x.Guidage ».
        bundles = req("GET", f"/bundleIds?filter[identifier]={identifier}&limit=200",
                      token)
        for b in bundles.get("data", []):
            if b["attributes"].get("identifier") == identifier:
                return b["id"]
        body = {"data": {"type": "bundleIds",
                         "attributes": {"identifier": identifier,
                                        "name": nom, "platform": "IOS"}}}
        created = req("POST", "/bundleIds", token, data=json.dumps(body))
        print(f"Bundle ID enregistré : {identifier}")
        return created["data"]["id"]

    bundle_uid = assure_bundle(BUNDLE_ID, "MonCap GPS")
    assure_bundle(f"{BUNDLE_ID}.Guidage", "MonCap GPS Guidage")
    # L'app montre a son propre bundle (l'API n'a pas de plateforme
    # watchOS distincte : IOS couvre aussi les apps de la montre).
    assure_bundle(f"{BUNDLE_ID}.watchkitapp", "MonCap GPS Montre")

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

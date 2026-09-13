#!/usr/bin/env python3
"""Crée dans App Store Connect le groupe d'abonnement et les deux formules
de MonCap — identifiants, textes vitrine dans les trois langues, et les
prix dans **tous** les territoires.

Pourquoi un script plutôt que l'interface : les identifiants doivent être
exacts au caractère près (`MonCapPlus.produitPlusID` les code en dur), et
un prix ne se pose pas en un clic — Apple demande un point de prix par
territoire, soit près de deux cents saisies par formule. La machine ne se
trompe pas de virgule et n'oublie pas la Corée.

La règle de la maison : **pas d'offre d'essai Apple**. L'essai de quatorze
jours est ancré côté serveur à la création du compte (voir MonCapPlus.swift)
— un essai StoreKit par-dessus le doublerait pour qui réinstalle.

Deux modes, par la variable MODE :
  - `verifie` (défaut) : ne crée rien, dit ce qui manque. Sans danger.
  - `cree`             : crée ce qui manque, et **seulement** ce qui manque.

Idempotent dans les deux sens : relancé, il ne recrée rien et ne modifie
aucun texte déjà posé — c'est l'interface d'Apple qui fait foi une fois
qu'un humain y a touché.

Dépendances : requests, pyjwt[crypto] (installées par le workflow).
"""

import os
import sys
import time

import jwt
import requests

API = "https://api.appstoreconnect.apple.com/v1"
BUNDLE_ID = os.environ.get("BUNDLE_ID", "com.maxlestage.moncap")
MODE = os.environ.get("MODE", "verifie")

# Le territoire de référence : le prix y est saisi tel quel, les autres en
# sont déduits par la table d'équivalence d'Apple (« equalizations »).
BASE = "FRA"

GROUPE = "MonCap"

# Les limites d'Apple sur les textes vitrine. Dépassées, l'API répond 409
# sans dire lequel — autant refuser ici, avec le nom du coupable.
MAX_NOM = 30
MAX_DESC = 45

# Niveau 1 = le plus haut du groupe. Pro au-dessus de Plus : c'est ce qui
# fait qu'un passage de Plus à Pro est une montée en gamme (immédiate, au
# prorata) et non un changement latéral.
FORMULES = [
    {
        "produit": "moncap.pro.mensuel",
        "reference": "MonCap Pro mensuel",
        "niveau": 1,
        "prix": "9.99",
        "textes": {
            "fr-FR": ("MonCap Pro", "Tout Plus + tournées, journal km, factures."),
            "es-ES": ("MonCap Pro", "Todo Plus + rutas, diario de km y facturas."),
            "en-US": ("MonCap Pro", "All of Plus + routes, mileage log, invoices."),
        },
    },
    {
        "produit": "moncap.plus.mensuel",
        "reference": "MonCap Plus mensuel",
        "niveau": 2,
        "prix": "3.99",
        "textes": {
            "fr-FR": ("MonCap Plus", "L'app entière : guidage, hors ligne, GPX."),
            "es-ES": ("MonCap Plus", "La app completa: guía, sin conexión, GPX."),
            "en-US": ("MonCap Plus", "The whole app: guidance, offline maps, GPX."),
        },
    },
]

NOTE_REVIEW = (
    "L'essai de quatorze jours est ancré côté serveur à la création du "
    "compte, pas par StoreKit : aucune offre d'essai Apple n'est attachée "
    "à ces produits, c'est voulu. Les deux formules partagent un groupe, "
    "Pro au niveau 1 et Plus au niveau 2."
)

faits = []
manques = []


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


class Asc:
    """Le strict nécessaire : GET paginé, POST, et un jeton qui se renouvelle.

    Le jeton vaut vingt minutes et la pose des prix en demande plusieurs
    centaines — il se refait tout seul passé un quart d'heure.
    """

    def __init__(self):
        self.session = requests.Session()
        self._token = None
        self._ne = 0

    def entete(self):
        if self._token is None or time.time() > self._ne:
            self._token = make_token()
            self._ne = time.time() + 900
        return {"Authorization": f"Bearer {self._token}"}

    def get(self, path):
        r = self.session.get(f"{API}{path}", headers=self.entete(), timeout=60)
        if r.status_code >= 300:
            die(f"GET {path} -> {r.status_code}: {r.text[:500]}")
        return r.json()

    def tout(self, path):
        """Suit les liens `next` : au-delà de deux cents, Apple pagine."""
        out = []
        suite = f"{API}{path}"
        while suite:
            r = self.session.get(suite, headers=self.entete(), timeout=60)
            if r.status_code >= 300:
                die(f"GET {suite} -> {r.status_code}: {r.text[:500]}")
            corps = r.json()
            out.extend(corps.get("data", []))
            suite = corps.get("links", {}).get("next")
        return out

    def post(self, path, corps):
        r = self.session.post(f"{API}{path}", headers=self.entete(), json=corps,
                              timeout=60)
        if r.status_code >= 300:
            die(f"POST {path} -> {r.status_code}: {r.text[:800]}")
        return r.json()


def lien(genre, ident):
    return {"data": {"type": genre, "id": ident}}


def groupe_dabonnement(asc, app_id):
    """Le groupe : un seul, partagé — c'est lui qui permet à Apple de gérer
    seul le passage de Plus à Pro sans double facturation."""
    for g in asc.tout(f"/apps/{app_id}/subscriptionGroups?limit=200"):
        if g["attributes"].get("referenceName") == GROUPE:
            return g["id"], False
    if MODE != "cree":
        manques.append(f"groupe d'abonnement « {GROUPE} »")
        return None, True
    corps = {
        "data": {
            "type": "subscriptionGroups",
            "attributes": {"referenceName": GROUPE},
            "relationships": {"app": lien("apps", app_id)},
        }
    }
    gid = asc.post("/subscriptionGroups", corps)["data"]["id"]
    faits.append(f"groupe « {GROUPE} » créé")
    return gid, True


def textes_du_groupe(asc, gid):
    """Le nom que le client voit dans ses abonnements iOS, par langue."""
    deja = {
        loc["attributes"]["locale"]
        for loc in asc.tout(
            f"/subscriptionGroups/{gid}/subscriptionGroupLocalizations?limit=200"
        )
    }
    for langue in ("fr-FR", "es-ES", "en-US"):
        if langue in deja:
            continue
        if MODE != "cree":
            manques.append(f"nom du groupe en {langue}")
            continue
        asc.post(
            "/subscriptionGroupLocalizations",
            {
                "data": {
                    "type": "subscriptionGroupLocalizations",
                    "attributes": {"name": GROUPE, "locale": langue},
                    "relationships": {"subscriptionGroup": lien("subscriptionGroups", gid)},
                }
            },
        )
        faits.append(f"nom du groupe posé en {langue}")


def abonnement(asc, gid, f):
    """Trouve la formule par son identifiant produit, ou la crée."""
    for s in asc.tout(
        f"/subscriptionGroups/{gid}/subscriptions?limit=200"
        "&fields[subscriptions]=name,productId,state,subscriptionPeriod,groupLevel"
    ):
        if s["attributes"].get("productId") == f["produit"]:
            return s["id"], s["attributes"].get("state")
    if MODE != "cree":
        manques.append(f"abonnement {f['produit']}")
        return None, None
    corps = {
        "data": {
            "type": "subscriptions",
            "attributes": {
                "name": f["reference"],
                "productId": f["produit"],
                "subscriptionPeriod": "ONE_MONTH",
                "familySharable": False,
                "groupLevel": f["niveau"],
                "reviewNote": NOTE_REVIEW,
            },
            "relationships": {"group": lien("subscriptionGroups", gid)},
        }
    }
    sid = asc.post("/subscriptions", corps)["data"]["id"]
    faits.append(f"{f['produit']} créé (niveau {f['niveau']}, mensuel)")
    return sid, "MISSING_METADATA"


def textes(asc, sid, f):
    """Nom et description vus sur la fiche App Store, par langue."""
    deja = {
        loc["attributes"]["locale"]
        for loc in asc.tout(f"/subscriptions/{sid}/subscriptionLocalizations?limit=200")
    }
    for langue, (nom, desc) in f["textes"].items():
        if len(nom) > MAX_NOM:
            die(f"{f['produit']} {langue} : nom de {len(nom)} signes (max {MAX_NOM})")
        if len(desc) > MAX_DESC:
            die(f"{f['produit']} {langue} : description de {len(desc)} signes "
                f"(max {MAX_DESC})")
        if langue in deja:
            continue
        if MODE != "cree":
            manques.append(f"textes de {f['produit']} en {langue}")
            continue
        asc.post(
            "/subscriptionLocalizations",
            {
                "data": {
                    "type": "subscriptionLocalizations",
                    "attributes": {"name": nom, "description": desc, "locale": langue},
                    "relationships": {"subscription": lien("subscriptions", sid)},
                }
            },
        )
        faits.append(f"textes de {f['produit']} posés en {langue}")


def prix(asc, sid, f):
    """Le prix, partout.

    Apple ne déduit rien : chaque territoire veut son propre point de prix.
    On saisit celui de France, puis on demande ses équivalents — la table
    d'Apple, pas une conversion maison, pour que les prix tombent juste
    (9,99 € devient 9,99 $, pas 10,73 $).
    """
    poses = {
        p["relationships"]["territory"]["data"]["id"]
        for p in asc.tout(f"/subscriptions/{sid}/prices?limit=200&include=territory")
        if p.get("relationships", {}).get("territory", {}).get("data")
    }
    if BASE in poses:
        return  # Déjà tarifé : on ne retouche pas un prix en vigueur.

    points = asc.tout(
        f"/subscriptions/{sid}/pricePoints?filter[territory]={BASE}&limit=200"
    )
    socle = next(
        (p for p in points if p["attributes"].get("customerPrice") == f["prix"]), None
    )
    if socle is None:
        die(f"{f['produit']} : aucun point de prix à {f['prix']} en {BASE}")

    cibles = [socle] + asc.tout(
        f"/subscriptionPricePoints/{socle['id']}/equalizations?limit=8000"
    )
    if MODE != "cree":
        manques.append(
            f"prix de {f['produit']} ({f['prix']} € en {BASE}, "
            f"{len(cibles)} territoires)"
        )
        return

    n = 0
    for point in cibles:
        territoire = (
            point.get("relationships", {}).get("territory", {}).get("data", {}).get("id")
        )
        if territoire and territoire in poses:
            continue
        asc.post(
            "/subscriptionPrices",
            {
                "data": {
                    "type": "subscriptionPrices",
                    "attributes": {"startDate": None, "preserveCurrentPrice": False},
                    "relationships": {
                        "subscription": lien("subscriptions", sid),
                        "subscriptionPricePoint": lien(
                            "subscriptionPricePoints", point["id"]
                        ),
                    },
                }
            },
        )
        n += 1
    faits.append(f"prix de {f['produit']} posé sur {n} territoires ({f['prix']} € en {BASE})")


def main():
    if MODE not in ("verifie", "cree"):
        die(f"MODE={MODE} inconnu (attendu : verifie ou cree)")
    asc = Asc()

    apps = asc.get(f"/apps?filter[bundleId]={BUNDLE_ID}").get("data", [])
    if not apps:
        die(f"Aucune app pour {BUNDLE_ID}.")
    app_id = apps[0]["id"]
    print(f"App {BUNDLE_ID} (id {app_id}) · mode {MODE}\n")

    gid, neuf = groupe_dabonnement(asc, app_id)
    if gid is None:
        # En vérification, sans groupe le reste ne peut pas être inspecté :
        # on dit ce qu'on sait et on s'arrête sans rien inventer.
        manques.extend(
            f"abonnement {f['produit']} (groupe absent)" for f in FORMULES
        )
    else:
        textes_du_groupe(asc, gid)
        for f in FORMULES:
            sid, etat = abonnement(asc, gid, f)
            if sid is None:
                continue
            print(f"  {f['produit']} · {etat}")
            textes(asc, sid, f)
            prix(asc, sid, f)

    print()
    for ligne in faits:
        print(f"  ✓ {ligne}")
    for ligne in manques:
        print(f"  · manque : {ligne}")
    if not faits and not manques:
        print("  Rien à faire : tout est déjà en place.")
    if manques and MODE == "cree":
        die("Des éléments manquent encore après création — voir ci-dessus.")

    print(
        "\nCe que la machine ne peut pas faire à ta place :\n"
        "  - la capture d'écran de review de chaque abonnement (obligatoire\n"
        "    avant soumission : l'écran « Ma formule » de l'app suffit) ;\n"
        "  - l'accord applications payantes et les coordonnées bancaires ;\n"
        "  - la demande au Small Business Program (15 % au lieu de 30 %)."
    )


if __name__ == "__main__":
    main()

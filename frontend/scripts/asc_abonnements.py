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

    def appel(self, methode, url, corps=None):
        """Un appel, avec la patience qu'exige l'API d'Apple.

        Observé en vrai sur cette app : un `GET .../subscriptionGroups`, la
        requête la plus banale qui soit, répond 500 UNEXPECTED_ERROR. Le
        défaut est chez eux et il passe tout seul — mais un script qui meurt
        au premier hoquet laisse un groupe créé, un abonnement créé, et pas
        de prix. On réessaie donc les 5xx, les 429 et les coupures réseau,
        en espaçant ; le reste (400, 403, 409) est une vraie réponse, qu'on
        rend telle quelle sans insister.
        """
        attente = 3
        dernier = None
        for essai in range(5):
            try:
                r = self.session.request(
                    methode, url, headers=self.entete(), json=corps, timeout=60
                )
            except requests.RequestException as e:
                dernier = (0, f"réseau : {e}")
            else:
                if r.status_code < 500 and r.status_code != 429:
                    return r.status_code, r
                dernier = (r.status_code, (r.text or "")[:400])
            if essai < 4:
                print(f"    Apple répond {dernier[0]}, nouvelle tentative "
                      f"dans {attente} s ({essai + 1}/4)…")
                time.sleep(attente)
                attente *= 2
        return dernier[0], dernier[1]

    def get(self, path):
        code, r = self.appel("GET", f"{API}{path}")
        if code >= 300 or isinstance(r, str):
            die(f"GET {path} -> {code}: {r if isinstance(r, str) else r.text[:500]}")
        return r.json()

    def tout(self, path):
        """Suit les liens `next` : au-delà de deux cents, Apple pagine."""
        out = []
        suite = f"{API}{path}"
        while suite:
            code, r = self.appel("GET", suite)
            if code >= 300 or isinstance(r, str):
                die(f"GET {suite} -> {code}: "
                    f"{r if isinstance(r, str) else r.text[:500]}")
            corps = r.json()
            out.extend(corps.get("data", []))
            suite = corps.get("links", {}).get("next")
        return out

    def post(self, path, corps):
        code, r = self.appel("POST", f"{API}{path}", corps)
        if code >= 300 or isinstance(r, str):
            die(f"POST {path} -> {code}: "
                f"{r if isinstance(r, str) else r.text[:800]}")
        return r.json()

    def tolere(self, path):
        """GET dont l'absence est une réponse : Apple rend 404 pour une
        relation qui n'a pas encore été posée, ce qui est le renseignement
        qu'on cherche, pas une panne."""
        code, r = self.appel("GET", f"{API}{path}")
        if code == 404:
            return None
        if code >= 300 or isinstance(r, str):
            die(f"GET {path} -> {code}: "
                f"{r if isinstance(r, str) else r.text[:500]}")
        return r.json()

    def essaie(self, path, corps):
        """Comme `post`, mais rend l'échec au lieu de mourir dessus.

        Les prix se posent par centaines : un territoire refusé ne doit pas
        emporter les cent quatre-vingts autres.
        """
        code, r = self.appel("POST", f"{API}{path}", corps)
        return code, (r if isinstance(r, str) else (r.text or "")[:400])


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


def disponibilite(asc, sid, f):
    """Ouvre la vente de l'abonnement, sans quoi aucun prix ne se pose.

    Diagnostic payé cher : neuf tentatives, trois écritures de requête, le
    même 409 ENTITY_ERROR.RELATIONSHIP.INVALID à chaque fois, sur un point
    de prix pourtant valide et rattaché à notre abonnement (son identifiant
    encode {"s": <abonnement>, "t": "FRA"}). Ce n'était donc ni un hoquet
    d'Apple, ni la forme de la requête.

    L'interface d'App Store Connect demande deux choses dans cet ordre — la
    disponibilité, puis les prix — et l'API tient le même ordre sans le
    dire : un abonnement neuf n'est vendu nulle part, et un prix pour la
    France n'a pas de sens tant que la France n'est pas ouverte. D'où un
    refus qui parle de relation invalide.
    """
    etat = asc.tolere(f"/subscriptions/{sid}/subscriptionAvailability")
    if etat is not None and etat.get("data"):
        return

    territoires = [t["id"] for t in asc.tout("/territories?limit=200")]
    if MODE != "cree":
        manques.append(
            f"disponibilité de {f['produit']} ({len(territoires)} territoires)"
        )
        return
    asc.post(
        "/subscriptionAvailabilities",
        {
            "data": {
                "type": "subscriptionAvailabilities",
                "attributes": {"availableInNewTerritories": True},
                "relationships": {
                    "subscription": lien("subscriptions", sid),
                    "availableTerritories": {
                        "data": [
                            {"type": "territories", "id": t} for t in territoires
                        ]
                    },
                },
            }
        },
    )
    faits.append(
        f"{f['produit']} ouvert à la vente dans {len(territoires)} territoires"
    )


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

    disponibilite(asc, sid, f)

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

    forme = pose_le_socle(asc, sid, socle, f)
    n = 1
    refuses = []
    for point in cibles[1:]:
        territoire = territoire_de(point)
        if territoire and territoire in poses:
            continue
        code, texte = asc.essaie("/subscriptionPrices", charge(sid, point["id"], forme))
        if code < 300:
            n += 1
        else:
            # Un territoire refusé n'emporte pas les autres : la plupart du
            # temps c'est un pays où l'app n'est pas distribuée.
            refuses.append(f"{territoire or '?'} ({code})")
    faits.append(
        f"prix de {f['produit']} posé sur {n} territoires ({f['prix']} € en {BASE})"
    )
    if refuses:
        print(f"    {len(refuses)} territoire(s) refusé(s) : {', '.join(refuses[:12])}"
              + (" …" if len(refuses) > 12 else ""))


def territoire_de(point):
    return point.get("relationships", {}).get("territory", {}).get("data", {}).get("id")


def charge(sid, point_id, attributs):
    corps = {
        "data": {
            "type": "subscriptionPrices",
            "relationships": {
                "subscription": lien("subscriptions", sid),
                "subscriptionPricePoint": lien("subscriptionPricePoints", point_id),
            },
        }
    }
    if attributs is not None:
        corps["data"]["attributes"] = attributs
    return corps


# Trois écritures possibles du même prix. Apple documente la première, mais
# la refuse parfois sur un abonnement tout juste créé : plutôt que parier,
# on essaie, et la forme qui passe sert pour les cent quatre-vingts autres.
FORMES = [
    ("startDate null + preserveCurrentPrice", {"startDate": None,
                                               "preserveCurrentPrice": False}),
    ("startDate null seul", {"startDate": None}),
    ("sans attributs", None),
]


def pose_le_socle(asc, sid, socle, f):
    """Pose le prix de référence, et rend la forme de requête qui a marché.

    Le socle est le seul prix dont l'échec est fatal : sans lui il n'y a
    pas de tarif du tout. On lui laisse donc trois écritures et trois
    tentatives espacées — l'abonnement vient d'être créé, et la tarification
    d'Apple met parfois quelques secondes à le connaître.
    """
    dernier = ""
    for essai in range(3):
        if essai:
            time.sleep(6)
        for nom, attributs in FORMES:
            code, texte = asc.essaie(
                "/subscriptionPrices", charge(sid, socle["id"], attributs)
            )
            if code < 300:
                if essai or nom != FORMES[0][0]:
                    print(f"    socle posé en « {nom} » (tentative {essai + 1})")
                return attributs
            dernier = f"{nom} -> {code}: {texte}"
        print(f"    socle refusé (tentative {essai + 1}/3), nouvelle tentative…")
    die(
        f"{f['produit']} : impossible de poser le prix de référence en {BASE}.\n"
        f"Point de prix {socle['id']} · {socle.get('attributes')}\n"
        f"Dernier refus : {dernier}\n"
        "Si le refus persiste après l'ouverture des territoires, la cause "
        "restante est hors API : l'accord applications payantes doit être "
        "signé et les coordonnées bancaires et fiscales renseignées — sans "
        "quoi Apple refuse tout tarif, et aucune clé ne signe un contrat."
    )


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

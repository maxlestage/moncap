# Le dossier App Store

Tout ce qui se colle dans App Store Connect, prêt à l'emploi. Les textes
suivent la règle de la maison : rien de promis que l'app ne fait pas.

## Identité

- **Nom** : MonCap GPS
- **Sous-titre** (30 car. max) :
  - FR : `Compare, roule, sans réseau`
  - ES : `Compara y conduce sin red`
  - EN : `Compare, drive, no signal`
- **Catégorie principale** : Navigation · **Secondaire** : Voyages
- **Âge** : 4+
- **Prix** : gratuite au téléchargement, abonnements intégrés
  (`moncap.plus.mensuel` 3,99 €/mois, `moncap.pro.mensuel` 9,99 €/mois,
  même groupe d'abonnement, **aucune offre d'essai Apple** — l'essai de
  quatorze jours est géré par le compte, côté serveur).

## Description (FR)

```
Un GPS qui compare avant de guider, et qui continue de calculer quand le
réseau tombe.

COMPARE, PUIS CHOISIS
Jusqu'à 15 itinéraires côte à côte : durée, distance, virages, dénivelé,
heure d'arrivée — et ce que ça coûte, carburant au prix réel des stations
autour de vous (open data officiel) et péages compris.

ROULE SANS RÉSEAU
212 régions dans 49 pays d'Europe, téléchargeables une par une. Les
itinéraires se calculent sur le téléphone : vallées, tunnels, étranger.
L'annuaire des lieux et les limites de vitesse descendent avec la région.

ET TOUT LE RESTE
• Guidage vocal, voies à prendre, vitesse comparée à la limite
• Transports en commun : lignes, correspondances, retards du moment
• Signalements communautaires, confirmés ou infirmés d'un geste
• Partage de position en direct par simple lien
• Siri, écran verrouillé et Dynamic Island
• La station-service la moins chère sur votre route
• Les zones à faibles émissions annoncées au choix de la route
• Mode moto — et ses « routes plaisir »
• Rappels d'entretien aux kilomètres réellement parcourus
• « Mon année » : votre journal en une image à partager

POUR CEUX QUI ROULENT POUR TRAVAILLER (Pro)
• Tournées optimisées — collées d'un bloc, une adresse par ligne
• Créneaux de livraison sur les étapes
• Gabarit utilitaire : les ponts bas s'écartent du trajet
• Journal kilométrique CSV, facturation PDF, attestation fiscale au barème

LE PRIX, DIT SIMPLEMENT
Quatorze jours d'essai, tout ouvert, pour tout le monde. Ensuite une
formule au mois : MonCap Plus (3,99 €) pour l'app entière, MonCap Pro
(9,99 €) pour les outils de ceux qui roulent pour travailler. Sans
engagement, résiliable dans les réglages iOS.
```

(ES et EN : reprendre la traduction des cartes du site — `vitrine/src/contenu.rs`
porte déjà chaque argument dans les trois langues, mêmes mots.)

## Mots-clés (100 car. max, FR)

```
gps,itinéraire,hors ligne,carte,navigation,tournée,livraison,carburant,vélo,moto,radar pluie,péage
```

## Nouveautés 1.2.1

```
• Deux formules : Plus (3,99 €/mois) et Pro (9,99 €/mois) — 14 jours
  d'essai, tout ouvert, pour tout le monde
• La station-service la moins chère sur votre route
• Tournées collées d'un bloc, créneaux de livraison, gabarit utilitaire
• Attestation fiscale au barème kilométrique
• ZFE annoncées, mode moto, rappels d'entretien, widget d'accueil
• « Mon année » : votre journal en une image
```

## Confidentialité (étiquettes « nutrition »)

Données **liées à l'utilisateur** (rattachées au compte) :
- Coordonnées → adresse e-mail (identifiant du compte, rien d'autre)
- Localisation → position précise (trajets enregistrés, partage live *choisi*)
- Contenu utilisateur → trajets, recherches, signalements, favoris

Données **non collectées** : identifiants publicitaires, historique de
navigation web, contacts, photos, santé, achats. **Aucun pistage** (répondre
« non » à App Tracking Transparency — il n'y a ni pub ni courtier).

Diagnostics → rapports de plantage (MetricKit, anonymes, sans identifiant).

La politique de confidentialité en ligne dit exactement cela :
`https://moncap-c41a5aaf07e8.herokuapp.com/confidentialite`.

## Notes pour la review Apple

```
Test account (required to pass the sign-in screen):
  email:    review.apple@moncap-gps.fr
  password: MonCap-Review-2026

About the paywall: every account gets a 14-day full-access trial anchored
server-side at account creation. After the trial the app locks behind a
subscription screen (restore & offer codes available). The two auto-
renewable subscriptions (moncap.plus.mensuel €3.99, moncap.pro.mensuel
€9.99) share one subscription group. There is deliberately no Apple
introductory offer: the trial is account-based.

Offline features need a region pack: on the demo account, open the menu →
« Hors ligne », download « France · Bretagne » (small) to test offline
routing, places and speed limits in airplane mode.

Location is used for navigation, trip recording and *user-initiated* live
sharing only. Crash diagnostics via MetricKit, anonymous.
```

⚠️ **Avant de soumettre** : ajouter `review.apple@moncap-gps.fr` à la
variable `PLUS_OFFERT` sur Heroku (Config Vars) — l'essai du compte de
test court depuis sa création, et une review qui arrive après quatorze
jours tomberait sur l'app verrouillée. Sur la liste d'invités, le
compte a tout, pour toujours.

L'action **Config Heroku** le fait sans risque d'effacer les invités déjà
présents (elle ajoute, elle ne remplace jamais) ; elle demande un secret
`HEROKU_API_KEY` dans le dépôt, obtenu par `heroku authorizations:create`.
Lance-la d'abord en `verifie`, elle dit ce qu'elle changerait.

## Captures d'écran (à faire sur simulateur iPhone 16 Pro Max + iPad)

L'ordre raconte l'app :
1. Le comparateur — plusieurs itinéraires, coûts affichés
2. Le guidage — bandeau, voies, vitesse vs limite
3. Le hors ligne — la liste des régions, une jauge en cours
4. La tournée — étapes, « Au plus court », un créneau orange
5. La station la moins chère — l'économie en euros
6. Le menu « Ma formule » — l'essai et les deux formules

## Check-list de soumission

Les lignes marquées *(action)* se font depuis l'onglet Actions du dépôt —
rien à saisir à la main, et rien d'irréversible : les deux actions ont un
mode « vérifie » qui ne touche à rien et dit ce qui manque.

- [ ] *(action **Abonnements App Store**, mode `cree`)* groupe « MonCap »
      et deux abonnements — identifiants `moncap.plus.mensuel` /
      `moncap.pro.mensuel`, mensuels, Pro au niveau 1 et Plus au niveau 2,
      textes en fr/es/en, 3,99 € et 9,99 € sur tous les territoires, sans
      offre d'essai Apple (l'essai est celui du compte, côté serveur)
- [ ] Capture d'écran de review pour **chacun** des deux abonnements
      (l'écran « Ma formule » convient) — Apple la refuse absente, et
      l'API ne sait pas la déposer
- [ ] Accord applications payantes signé + coordonnées bancaires/fiscales
- [ ] Small Business Program demandé (15 % de commission au lieu de 30 %)
- [ ] *(action **Config Heroku**, mode `applique`)* `PLUS_OFFERT` ←
      compte de review (voir ⚠️ ci-dessus)
- [ ] Étiquettes de confidentialité remplies comme ci-dessus
- [ ] Captures d'écran ×3 langues (ou FR partout : accepté)
- [ ] Build 1.2.1+ sélectionné, notes de review collées

## Référencement : ce qui attend un jeton ou un domaine

Le site est prêt des deux côtés, il ne manque qu'une valeur à chaque fois —
la construction (`vitrine/src/bin/construit.rs`) lit deux variables d'environnement :

- `GOOGLE_SITE_VERIFICATION` : le jeton de Google Search Console. Posé, la
  balise `<meta name="google-site-verification">` apparaît sur les neuf
  pages ; absent, aucune balise n'est écrite (une balise vide se lirait
  comme une vérification ratée). Une fois le site vérifié, le plan est
  déjà là : `/sitemap.xml`, neuf pages, hreflang compris.
- `SITE_PUBLIC` : l'adresse canonique. Le jour où le vrai nom de domaine
  arrive, cette seule variable déplace la canonique, les hreflang, le plan
  du site, Open Graph et les données structurées — d'un coup, sans toucher
  au code. Défaut : l'adresse Heroku actuelle.

# MonCap GPS — App iOS (test sur iPhone)

Guide pas-à-pas pour ouvrir, lancer et tester l'application sur un iPhone
réel, avec **Xcode sur Mac**. Aucune ligne de commande nécessaire.

## Prérequis

- Un **Mac** avec **Xcode 16** ou plus récent (App Store, gratuit).
- Un **iPhone** sous **iOS 17** ou plus (le projet cible iOS 17).
- Un câble pour brancher l'iPhone au Mac (ou Wi-Fi même réseau).
- Un **identifiant Apple**. Un **compte développeur payant** permet en plus
  la distribution **TestFlight** (voir §7) — idéal si tu n'as le Mac que le
  week-end : tu construis une fois, puis tu testes sur ton iPhone toute la
  semaine **sans le Mac**.

## 1. Récupérer le projet

Le code est sur la branche `master` du dépôt. Sur le Mac :
télécharge le ZIP du dépôt depuis GitHub (bouton **Code → Download ZIP**),
décompresse, puis ouvre **`frontend/MonCapGPS.xcodeproj`** dans Xcode.

## 2. Configurer la signature (une seule fois)

1. Dans Xcode, sélectionne le projet **MonCapGPS** (icône bleue en haut à
   gauche) → onglet **Signing & Capabilities**.
2. Coche **Automatically manage signing**.
3. **Team** : choisis ton identifiant Apple (clique *Add an Account…* si
   besoin et connecte-toi).
4. Si le *Bundle Identifier* `com.maxlestage.moncap` est déjà pris, change-le
   pour quelque chose d'unique, ex. `com.tonprenom.moncap`.

## 3. Lancer sur l'iPhone

1. Branche l'iPhone. En haut de Xcode, choisis-le comme destination (à côté
   du bouton ▶︎).
2. Sur l'iPhone : **Réglages → Confidentialité et sécurité → Mode développeur**
   → active-le (l'iPhone redémarre).
3. Clique **▶︎ (Run)** dans Xcode.
4. Au 1er lancement, l'app sera bloquée : **Réglages → Général → VPN et gestion
   de l'appareil** → fais confiance à ton certificat de développeur.
5. Relance avec ▶︎.

> Tu peux aussi tester dans le **simulateur** iPhone (pas besoin de signature),
> mais le GPS y est simulé : pour la vraie position et la vitesse, utilise un
> iPhone réel.

## 4. Autorisations

Au démarrage, l'app demande l'accès à la **localisation** : accepte
(*Lorsque l'app est active*). C'est indispensable pour la position, les
trajets, la navigation et le partage en direct.

## 5. Se connecter

L'app utilise le backend déjà déployé :
`https://moncap-c41a5aaf07e8.herokuapp.com`.

- Sur l'écran de connexion, choisis **Pas de compte ? S'inscrire**, crée un
  identifiant + mot de passe, et c'est parti.

## 6. (Recommandé) Distribuer via TestFlight — tester sans le Mac

Tu as un compte développeur Apple : construis l'app **une fois** sur le Mac
ce week-end, envoie-la sur TestFlight, puis installe-la et teste-la depuis
ton iPhone **toute la semaine, sans le Mac**.

1. **Créer la fiche app** sur [App Store Connect](https://appstoreconnect.apple.com)
   → *Apps* → **＋** → *Nouvelle app* :
   - Plateforme **iOS**, nom (ex. « MonCap GPS »), langue, et **Bundle ID**
     `com.maxlestage.moncap` (le même que dans Xcode ; crée-le d'abord dans
     *Certificates, Identifiers & Profiles* s'il n'apparaît pas).
   - SKU : n'importe quoi d'unique (ex. `moncap`).
2. **Archiver** dans Xcode :
   - Destination en haut → **Any iOS Device (arm64)**.
   - Menu **Product → Archive** (attends la fin de la compilation).
3. Dans l'**Organizer** qui s'ouvre : **Distribute App** →
   *App Store Connect* → *Upload* → laisse les options par défaut (signature
   automatique) → **Upload**.
4. Patiente ~10–30 min : le build apparaît dans App Store Connect →
   onglet **TestFlight**. (La clé *export compliance* est déjà réglée dans le
   projet, donc pas de question à ce sujet.)
5. Ajoute-toi comme testeur : **TestFlight → Test interne → ＋** → ton adresse.
6. Sur l'iPhone : installe l'app **TestFlight** (App Store), connecte-toi avec
   le même identifiant Apple, et installe **MonCap GPS**. Tu peux maintenant
   tester quand tu veux, sans le Mac. 🚐

> Chaque nouvelle version : incrémente *Build* (ex. 1 → 2) dans Xcode, puis
> refais **Archive → Upload**.

## 6 bis. (Automatique) Upload TestFlight via GitHub Actions — sans Mac

Un pipeline CI (`.github/workflows/ios-testflight.yml` + `fastlane/`) archive,
signe et envoie l'app sur TestFlight depuis un runner macOS — **aucun Mac
requis de ton côté**. Configuration en **une seule fois**, au navigateur :

### a) Créer une clé App Store Connect API

1. [App Store Connect](https://appstoreconnect.apple.com) → **Users and Access**
   → onglet **Integrations** (ou *Keys*) → **App Store Connect API**.
2. Génère une clé avec le rôle **App Manager**.
3. Note l'**Issuer ID** (en haut) et le **Key ID**, puis **télécharge le
   fichier `.p8`** (⚠️ téléchargeable une seule fois).

### b) Créer la fiche de l'app (une fois)

App Store Connect → **Apps → ＋ → Nouvelle app** : plateforme iOS, nom, langue,
**Bundle ID** `com.maxlestage.moncap`, SKU au choix. (Sinon le tout premier
upload échouera, faute d'app existante.)

### c) Ajouter les secrets GitHub

Dépôt GitHub → **Settings → Secrets and variables → Actions → New repository
secret**. Crée exactement ces trois secrets :

| Nom | Valeur |
|-----|--------|
| `ASC_KEY_ID` | le **Key ID** de l'étape (a) |
| `ASC_ISSUER_ID` | l'**Issuer ID** de l'étape (a) |
| `ASC_KEY_CONTENT` | le contenu du `.p8` **encodé en base64** (voir ci-dessous) |

Pour obtenir le base64 du `.p8` **sans Mac/terminal** : ouvre
[base64.guru/converter/encode/file](https://base64.guru/converter/encode/file),
choisis ton fichier `.p8`, copie la sortie et colle-la dans `ASC_KEY_CONTENT`.
*(Ou, si tu as un terminal : `base64 -i AuthKey_XXXX.p8 | pbcopy`.)*

### d) Builds automatiques (et manuels)

Une fois les 3 secrets en place, **c'est automatique** : chaque changement de
l'app iOS mergé dans `master` (fichiers `frontend/MonCapGPS/**`, le projet
Xcode ou la config fastlane) déclenche un build + upload TestFlight, sans
rien faire. Le numéro de build s'incrémente tout seul à chaque fois.

Tu peux aussi le lancer :
- **à la main** : onglet **Actions → iOS TestFlight → Run workflow** ;
- **par un tag** : pousse `ios-v1`, `ios-v2`, …

> Tant que les secrets ne sont pas configurés, le workflow ne fait rien (il
> s'arrête proprement avec un avertissement, sans échec rouge).
>
> Un build touchant seulement le backend, le web ou la doc **ne déclenche pas**
> de build iOS (pour économiser les minutes macOS, facturées ×10).

Ensuite : App Store Connect → **TestFlight** → ajoute-toi en testeur interne,
installe l'app **TestFlight** sur l'iPhone, et teste.

## 7. À tester ce week-end

- [ ] **Inscription / connexion** (puis déconnexion via la feuille « Lieux »).
- [ ] **Position en direct** : ton point bleu sur la carte, la **vitesse**
      (pastille km/h) qui évolue en voiture.
- [ ] **Enregistrer ma position** (feuille « Lieux » → bouton).
- [ ] **Itinéraire le plus simple en vert** entre tes points enregistrés.
- [ ] **Navigation turn-by-turn** : appuie sur une destination → « Y aller »,
      voix en français, bannière de virage, ETA.
- [ ] **Recalcul auto** : sors volontairement de l'itinéraire → l'app
      recalcule (bannière orange + « Recalcul de l'itinéraire »).
- [ ] **Partage en direct** (bouton « Partager ») : ta voiture (avatar)
      visible en temps réel ; teste à deux appareils/comptes.
- [ ] **Choix de l'avatar** (feuille « Lieux » → section « Mon avatar »).
- [ ] **Signalements** façon Waze (bouton orange) : police, accident,
      bouchon, danger — visibles en direct.
- [ ] **Export GPX** (feuille « Lieux » → bouton GPX → partage).

## Dépannage rapide

- **« authentification requise » / 401** : reconnecte-toi (le jeton a pu
  expirer) — re-saisis identifiant/mot de passe.
- **Pas de position** : vérifie l'autorisation de localisation
  (Réglages → MonCap GPS → Localisation) et que tu es à l'extérieur / GPS actif.
- **Rien ne se charge** : vérifie ta connexion ; le backend Heroku peut mettre
  ~5 s à « se réveiller » au tout premier appel.
- **Pas de voix en navigation** : monte le volume et désactive le mode
  silencieux (la voix est configurée pour passer même en arrière-plan audio).

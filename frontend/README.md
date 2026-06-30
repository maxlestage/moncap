# MonCap GPS — App iOS (test sur iPhone)

Guide pas-à-pas pour ouvrir, lancer et tester l'application sur un iPhone
réel, avec **Xcode sur Mac**. Aucune ligne de commande nécessaire.

## Prérequis

- Un **Mac** avec **Xcode 16** ou plus récent (App Store, gratuit).
- Un **iPhone** sous **iOS 17** ou plus (le projet cible iOS 17).
- Un câble pour brancher l'iPhone au Mac (ou Wi-Fi même réseau).
- Un **identifiant Apple** (gratuit suffit pour tester 7 jours sur ton
  propre appareil — pas besoin du compte développeur payant).

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

## 6. À tester ce week-end

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

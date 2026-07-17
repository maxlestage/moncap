# MonCap GPS — Fiche de tests & retours

À remplir au fil des tests sur iPhone. Coche les cases, note ce qui cloche,
et reporte les bugs en bas. Tu peux m'envoyer ce fichier rempli (ou juste
les parties qui posent problème) et je corrige.

Légende : ✅ OK · ⚠️ moyen / à revoir · ❌ KO / bug

---

## Contexte du test

- Date : ………………………………
- iPhone (modèle) : ………………………………
- Version iOS : ………………………………
- Build testé (n° dans TestFlight) : ………………………………
- Lieu / conditions (à pied, en voiture, ville/campagne) : ………………………………

---

## 1. Installation & lancement

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 1.1 | Installation via TestFlight sans erreur |  |  |
| 1.2 | L'app s'ouvre sans planter |  |  |
| 1.3 | Demande d'autorisation de localisation affichée |  |  |
| 1.4 | Icône et nom de l'app corrects sur l'écran d'accueil |  |  |

## 2. Compte (inscription / connexion)

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 2.1 | Création de compte (S'inscrire) |  |  |
| 2.2 | Message d'erreur clair si identifiants invalides |  |  |
| 2.3 | Connexion avec le compte créé |  |  |
| 2.4 | Reste connecté après fermeture/réouverture de l'app |  |  |
| 2.5 | Déconnexion (feuille « Lieux » → Compte) |  |  |

## 3. Carte & position

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 3.1 | Mon point bleu apparaît au bon endroit |  |  |
| 3.2 | Bouton « recentrer » ramène sur ma position |  |  |
| 3.3 | Pastille de **vitesse** (km/h) s'affiche et évolue en voiture |  |  |
| 3.4 | Pastille de connexion temps réel (point vert) |  |  |
| 3.5 | Boussole / orientation de la carte |  |  |

## 4. Lieux enregistrés

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 4.1 | « Enregistrer ma position » ajoute un point |  |  |
| 4.2 | Les points enregistrés réapparaissent après relance |  |  |
| 4.3 | Suppression d'un point (balayer pour supprimer) |  |  |
| 4.4 | Statistiques (nb de positions, total km) cohérentes |  |  |

## 5. Itinéraire le plus simple (vert)

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 5.1 | « Itinéraire le plus simple » trace une ligne **verte** |  |  |
| 5.2 | Le résumé (km · min) s'affiche |  |  |
| 5.3 | La carte se cadre sur l'itinéraire |  |  |

## 6. Navigation turn-by-turn

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 6.1 | « Y aller » depuis une destination démarre la nav |  |  |
| 6.2 | Bannière de virage + distance au prochain virage |  |  |
| 6.3 | **Voix en français** aux bons moments |  |  |
| 6.4 | ETA (min) et km restants cohérents |  |  |
| 6.5 | La carte suit et tourne dans le sens de la marche |  |  |
| 6.6 | **Recalcul auto** si je sors de l'itinéraire (bannière orange) |  |  |
| 6.7 | Annonce « arrivé à destination » |  |  |
| 6.8 | Bouton « Quitter » arrête bien la navigation |  |  |

## 7. Temps réel & partage

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 7.1 | Bouton « Partager » active le partage de ma position |  |  |
| 7.2 | Ma voiture (avatar) visible par un autre compte/appareil |  |  |
| 7.3 | Les autres voitures apparaissent/disparaissent en direct |  |  |
| 7.4 | « Stop partage » arrête bien le partage |  |  |

## 8. Avatars

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 8.1 | La liste d'avatars s'affiche (combis, F1, Yaris, Abarth) |  |  |
| 8.2 | Le choix est bien pris en compte sur la carte |  |  |
| 8.3 | L'avatar est conservé après relance |  |  |

## 9. Signalements (communautaires)

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 9.1 | Le bouton orange ouvre la grille de signalements |  |  |
| 9.2 | Police / Accident / Bouchon / Danger s'envoient |  |  |
| 9.3 | Le signalement apparaît sur la carte (emoji) |  |  |
| 9.4 | Visible aussi par un autre appareil/compte |  |  |

## 10. Export GPX

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 10.1 | Bouton « GPX » génère le fichier |  |  |
| 10.2 | La feuille de partage iOS s'ouvre |  |  |
| 10.3 | Le fichier s'ouvre dans une autre app (Fichiers, Mail…) |  |  |

## 11. Ressenti général

| # | À vérifier | État | Notes |
|---|------------|:----:|-------|
| 11.1 | Fluidité (pas de ralentissements) |  |  |
| 11.2 | Lisibilité / taille des textes et boutons |  |  |
| 11.3 | Consommation batterie raisonnable |  |  |
| 11.4 | Comportement avec mauvaise connexion |  |  |

---

## 🐞 Bugs rencontrés

> Copie ce bloc autant de fois que nécessaire.

### Bug #1
- **Titre court** : ………………………………
- **Où** (écran/bouton) : ………………………………
- **Étapes pour reproduire** :
  1. ………………………………
  2. ………………………………
  3. ………………………………
- **Résultat attendu** : ………………………………
- **Résultat obtenu** : ………………………………
- **Gravité** : ❌ bloquant · ⚠️ gênant · 💬 cosmétique
- **Reproductible ?** : toujours / parfois / une fois
- **Capture d'écran** : (joindre si possible)

### Bug #2
- **Titre court** : ………………………………
- **Où** : ………………………………
- **Étapes** :
  1. ………………………………
- **Attendu** : ………………………………
- **Obtenu** : ………………………………
- **Gravité** : ❌ / ⚠️ / 💬
- **Reproductible ?** : ………………………………

---

## 💡 Idées / améliorations souhaitées

- ………………………………
- ………………………………
- ………………………………

---

## ✋ Priorités pour la prochaine itération

1. ………………………………
2. ………………………………
3. ………………………………

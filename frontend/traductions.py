#!/usr/bin/env python3
"""Génère `MonCapGPS/Localizable.xcstrings` à partir du tableau ci-dessous.

Le français est la langue source : chaque clé **est** la chaîne française
écrite dans le code Swift. SwiftUI la cherche dans le catalogue et retombe
sur la clé elle-même quand la traduction manque — une entrée oubliée donne
donc du français, jamais un trou ni un plantage.

Écrire le tableau ici plutôt que le JSON à la main : le format `.xcstrings`
est verbeux et se relit mal, alors qu'une colonne par langue se vérifie d'un
coup d'œil. Relancer après toute modification :

    python3 frontend/traductions.py
"""

import glob
import json
import pathlib
import re
import sys

# clé française -> (espagnol, anglais)
# Ne sont listées que les chaînes qui changent : « GPX », « SP95 » ou
# « MonCap GPS » s'écrivent pareil partout et n'ont rien à faire ici.
T: dict[str, tuple[str, str]] = {
    # --- Actions et boutons ---
    "Abandonner": ("Abandonar", "Discard"),
    "Annuler": ("Cancelar", "Cancel"),
    "Fermer": ("Cerrar", "Close"),
    "Quitter": ("Salir", "Exit"),
    "Démarrer": ("Empezar", "Start"),
    "Reprendre": ("Reanudar", "Resume"),
    "Effacer": ("Borrar", "Clear"),
    "Tout effacer": ("Borrar todo", "Clear all"),
    "Réessayer": ("Reintentar", "Retry"),
    "Signaler": ("Avisar", "Report"),
    "Y aller": ("Ir", "Go"),
    "Détail": ("Detalle", "Details"),
    "Déconnexion": ("Cerrar sesión", "Sign out"),
    "Enregistrer ma position": ("Guardar mi posición", "Save my position"),
    "Redéfinir sur ma position": ("Fijar en mi posición", "Set to my position"),
    "Envoyer le message": ("Enviar el mensaje", "Send the message"),
    "Renvoyer le lien de suivi": ("Reenviar el enlace de seguimiento", "Resend the tracking link"),
    "Partager ma position en direct": ("Compartir mi posición en directo", "Share my position live"),
    "Arrêter le partage en direct": ("Dejar de compartir en directo", "Stop sharing live"),
    "Prévenir que je suis arrivé": ("Avisar de que he llegado", "Let them know I've arrived"),
    "Prévenir un proche": ("Avisar a un allegado", "Let someone know"),
    "Naviguer hors ligne": ("Navegar sin conexión", "Navigate offline"),
    "Revenir au fond par défaut": ("Volver al fondo por defecto", "Back to the default background"),
    "Obtenir une clé": ("Conseguir una clave", "Get a key"),
    "Obtenir une clé (2 500 req./jour)": (
        "Conseguir una clave (2 500 pet./día)",
        "Get a key (2,500 req./day)",
    ),
    "Supprimer mon compte": ("Eliminar mi cuenta", "Delete my account"),
    "Tout supprimer définitivement": ("Eliminarlo todo definitivamente", "Delete everything permanently"),
    "👍 Toujours là": ("👍 Sigue ahí", "👍 Still there"),
    "👎 Plus là": ("👎 Ya no está", "👎 Gone"),
    # --- Titres et sections ---
    "Mes trajets": ("Mis viajes", "My trips"),
    "Destinations": ("Destinos", "Destinations"),
    "Favoris": ("Favoritos", "Favourites"),
    "Récentes": ("Recientes", "Recent"),
    "Suggestions": ("Sugerencias", "Suggestions"),
    "Alertes": ("Avisos", "Reports"),
    "Alertes à proximité": ("Avisos cercanos", "Nearby reports"),
    "Statistiques": ("Estadísticas", "Statistics"),
    "Compte": ("Cuenta", "Account"),
    "Données": ("Datos", "Data"),
    "Mode": ("Modo", "Mode"),
    "Message": ("Mensaje", "Message"),
    "Ton message": ("Tu mensaje", "Your message"),
    "Destinataire": ("Destinatario", "Recipient"),
    "Mon avatar": ("Mi avatar", "My avatar"),
    "Avatar actuel": ("Avatar actual", "Current avatar"),
    "Mon véhicule": ("Mi vehículo", "My vehicle"),
    "Carburant": ("Combustible", "Fuel"),
    "Consommation": ("Consumo", "Consumption"),
    "Prix carburant": ("Precio del combustible", "Fuel price"),
    "Carte hors ligne": ("Mapa sin conexión", "Offline map"),
    "Carte des feux de forêt": ("Mapa de incendios forestales", "Wildfire map"),
    "Trafic 🚦 (TomTom)": ("Tráfico 🚦 (TomTom)", "Traffic 🚦 (TomTom)"),
    "Incendies 🔥 (NASA FIRMS)": ("Incendios 🔥 (NASA FIRMS)", "Wildfires 🔥 (NASA FIRMS)"),
    "Serveur de cartes personnel": ("Servidor de mapas propio", "Personal map server"),
    "Classement des contributeurs": ("Clasificación de contribuyentes", "Contributor leaderboard"),
    "Politique de confidentialité": ("Política de privacidad", "Privacy policy"),
    "À propos": ("Acerca de", "About"),
    "Navigation en temps réel": ("Navegación en tiempo real", "Real-time navigation"),
    "par Maxime Nathan Lestage": ("por Maxime Nathan Lestage", "by Maxime Nathan Lestage"),
    # --- Champs et invites ---
    "Où allez-vous ?": ("¿Adónde vas?", "Where are you going?"),
    "Chercher un lieu, une ville, un pays": (
        "Buscar un lugar, una ciudad, un país",
        "Search a place, a city, a country",
    ),
    "Hors ligne 🗺️": ("Sin conexión 🗺️", "Offline 🗺️"),
    "Préparer une zone hors ligne": (
        "Preparar una zona sin conexión",
        "Prepare an offline area",
    ),
    "Les fonds de carte et le calcul d'itinéraire se téléchargent ensemble, sur la zone que tu regardes. Aucune clé n'est nécessaire : tout vient d'OpenStreetMap, libre et sans inscription.": (
        "Los fondos de mapa y el cálculo de rutas se descargan juntos, sobre la zona que estás mirando. No hace falta ninguna clave: todo viene de OpenStreetMap, libre y sin registro.",
        "Map backgrounds and route computation download together, for the area you are looking at. No key is needed: everything comes from OpenStreetMap, open and without signing up.",
    ),
    "Itinéraires hors réseau": ("Rutas sin conexión", "Offline routes"),
    "Toutes les régions": ("Todas las regiones", "All regions"),
    "Sur cet appareil": ("En este dispositivo", "On this device"),
    "Déjà installée": ("Ya instalada", "Already installed"),
    "Mise à jour disponible · %@": (
        "Actualización disponible · %@",
        "Update available · %@",
    ),
    "Calcul sur l'appareil · %@": (
        "Cálculo en el dispositivo · %@",
        "Computed on the device · %@",
    ),
    "Couvre ta position": ("Cubre tu posición", "Covers your location"),
    "Aucune région installée ne répond à cet endroit.": (
        "Ninguna región instalada responde en este lugar.",
        "No installed region answers at this location.",
    ),
    "Aucune région reconnue ici. Ouvre la liste complète.": (
        "Ninguna región reconocida aquí. Abre la lista completa.",
        "No region recognised here. Open the full list.",
    ),
    "Téléchargement · %lld %": ("Descarga · %lld %", "Download · %lld %"),
    "%lld région(s) · %@": ("%lld región(es) · %@", "%lld region(s) · %@"),
    "Permet de calculer et de recalculer un itinéraire sans réseau. Garde autant de régions que tu veux : un trajet cherche sa route dans chacune. En revanche il doit tenir entier dans l'une d'elles — c'est à cela que servent les zones, qui se chevauchent.": (
        "Permite calcular y recalcular una ruta sin conexión. Guarda tantas regiones como quieras: un trayecto busca su ruta en cada una. En cambio debe caber entero en una de ellas: para eso sirven las zonas, que se solapan.",
        "Lets you compute and recompute a route with no network. Keep as many regions as you like: a trip looks for its route in each one. It must however fit entirely within one of them — that is what the overlapping areas are for.",
    ),
    "Le serveur a refusé la liste (HTTP %lld).": (
        "El servidor rechazó la lista (HTTP %lld).",
        "The server refused the list (HTTP %lld).",
    ),
    "Le serveur ne connaît pas cette adresse — backend à mettre à jour.": (
        "El servidor no conoce esta dirección: hay que actualizar el backend.",
        "The server does not know this address — the backend needs updating.",
    ),
    "Fonds de carte": ("Fondos de mapa", "Map backgrounds"),
    "%@ · %.0f Mo sur %.0f Mo": (
        "%@ · %.0f MB de %.0f MB",
        "%@ · %.0f MB of %.0f MB",
    ),
    "Téléchargement interrompu.": (
        "Descarga interrumpida.",
        "Download interrupted.",
    ),
    "Sans réseau": ("Sin cobertura", "No network"),
    "La recherche d'adresse a besoin du réseau. Appuie longuement sur la carte pour choisir un point : l'itinéraire se calcule alors sur l'appareil.": (
        "La búsqueda de direcciones necesita cobertura. Mantén pulsado el mapa para elegir un punto: la ruta se calcula entonces en el dispositivo.",
        "Address search needs a network connection. Press and hold the map to pick a point: the route is then computed on the device.",
    ),
    "Aucun itinéraire": ("Ninguna ruta", "No route"),
    "Étape": ("Parada", "Stop"),
    "Touche la carte": ("Toca el mapa", "Tap the map"),
    "Étape %d": ("Parada %d", "Stop %d"),
    "Au plus court": ("El más corto", "Shortest order"),
    "Partager l'itinéraire": ("Compartir la ruta", "Share the route"),
    "Rentrer à la maison": ("Volver a casa", "Go home"),
    "Aller au travail": ("Ir al trabajo", "Go to work"),
    "Itinéraire vers": ("Ruta hacia", "Route to"),
    "Destination": ("Destino", "Destination"),
    "Maison": ("Casa", "Home"),
    "Travail": ("Trabajo", "Work"),
    "Définis d'abord ton domicile dans les favoris de MonCap GPS.": (
        "Define primero tu casa en los favoritos de MonCap GPS.",
        "First set your home in MonCap GPS favourites.",
    ),
    "Définis d'abord ton travail dans les favoris de MonCap GPS.": (
        "Define primero tu trabajo en los favoritos de MonCap GPS.",
        "First set your work in MonCap GPS favourites.",
    ),
    "C'est parti vers la maison.": ("En marcha hacia casa.", "On the way home."),
    "C'est parti vers le travail.": ("En marcha hacia el trabajo.", "On the way to work."),
    "Je n'ai pas trouvé cette adresse — il faut du réseau pour chercher un nom de lieu.": (
        "No encontré esa dirección — hace falta red para buscar un nombre de lugar.",
        "I couldn't find that address — network is needed to look up a place name.",
    ),
    "Voilà les itinéraires.": ("Aquí están las rutas.", "Here are the routes."),
    # --- MonCap Plus (abonnement) ---
    "MonCap Plus": ("MonCap Plus", "MonCap Plus"),
    "Ma formule": ("Mi fórmula", "My plan"),
    "Ton essai est terminé": ("Tu prueba ha terminado", "Your trial is over"),
    "Choisis ta formule": ("Elige tu fórmula", "Choose your plan"),
    "Il reste %d jour(s) d'essai — tout est ouvert.": (
        "Quedan %d día(s) de prueba — todo está abierto.",
        "%d trial day(s) left — everything is open.",
    ),
    "Quatorze jours pour tout essayer, puis une formule — tout est payant, c'est la règle.": (
        "Catorce días para probarlo todo, después una fórmula — todo es de pago, es la regla.",
        "Fourteen days to try everything, then a plan — everything is paid, that's the rule.",
    ),
    "L'app entière : guidage, comparateur, transports": (
        "La app entera: guía, comparador, transportes",
        "The whole app: guidance, comparison, transit",
    ),
    "212 régions hors ligne": ("212 regiones sin conexión", "212 offline regions"),
    "Import de traces GPX": ("Importación de trazas GPX", "GPX track import"),
    "Tout MonCap Plus": ("Todo MonCap Plus", "Everything in MonCap Plus"),
    "Journal kilométrique exportable (CSV)": (
        "Registro de kilómetros exportable (CSV)",
        "Exportable mileage log (CSV)",
    ),
    "Facturation PDF des trajets": (
        "Facturación PDF de los trayectos",
        "PDF invoicing of your trips",
    ),
    "%@ / mois": ("%@ / mes", "%@ / month"),
    "Choisir": ("Elegir", "Choose"),
    "Ta formule actuelle": ("Tu fórmula actual", "Your current plan"),
    "Tout offert 🎁": ("Todo regalado 🎁", "Everything on the house 🎁"),
    "MonCap Pro actif": ("MonCap Pro activo", "MonCap Pro active"),
    "MonCap Plus actif": ("MonCap Plus activo", "MonCap Plus active"),
    "Essai en cours — tout est ouvert": (
        "Prueba en curso — todo está abierto",
        "Trial running — everything is open",
    ),
    "Il reste %d jour(s), puis une formule.": (
        "Quedan %d día(s), después una fórmula.",
        "%d day(s) left, then a plan.",
    ),
    "Choisir ma formule": ("Elegir mi fórmula", "Choose my plan"),
    "Plus %@ ou Pro %@ par mois.": (
        "Plus %@ o Pro %@ al mes.",
        "Plus %@ or Pro %@ per month.",
    ),
    "Tournées optimisées": ("Rutas de reparto optimizadas", "Optimized multi-stop runs"),
    "Import de traces GPX": ("Importación de trazas GPX", "GPX track import"),
    "La boutique ne répond pas — réessaie avec du réseau.": (
        "La tienda no responde — reintenta con cobertura.",
        "The store isn't responding — try again with signal.",
    ),
    "Restaurer les achats": ("Restaurar compras", "Restore purchases"),
    "J'ai un code": ("Tengo un código", "I have a code"),
    "Renouvellement le %@.": ("Renovación el %@.", "Renews on %@."),
    "Gérer l'abonnement": ("Gestionar la suscripción", "Manage subscription"),
    "Abonnement mensuel, renouvelé automatiquement, résiliable à tout moment dans les réglages iOS.": (
        "Suscripción mensual, renovada automáticamente, cancelable en cualquier momento en los ajustes de iOS.",
        "Monthly subscription, renews automatically, cancel anytime in iOS settings.",
    ),
    "Confidentialité": ("Privacidad", "Privacy"),
    "Conditions": ("Condiciones", "Terms"),
    # --- Journal et facturation ---
    "Exporter en CSV": ("Exportar en CSV", "Export as CSV"),
    "Facturation": ("Facturación", "Invoicing"),
    "Date;Trajet;Distance (km);Durée (min)": (
        "Fecha;Trayecto;Distancia (km);Duración (min)",
        "Date;Trip;Distance (km);Duration (min)",
    ),
    "Émetteur": ("Emisor", "Issuer"),
    "Nom ou entreprise": ("Nombre o empresa", "Name or company"),
    "Adresse": ("Dirección", "Address"),
    "SIRET (optionnel)": ("NIF (opcional)", "Company ID (optional)"),
    "Facture": ("Factura", "Invoice"),
    "Client": ("Cliente", "Client"),
    "Mois facturé": ("Mes facturado", "Invoiced month"),
    "Taux par kilomètre": ("Tarifa por kilómetro", "Rate per kilometre"),
    "Mention libre (TVA, franchise…)": (
        "Mención libre (IVA, franquicia…)", "Free-form note (VAT, exemption…)"),
    "%d trajet(s) sur la période": (
        "%d trayecto(s) en el período", "%d trip(s) in the period"),
    "Générer la facture PDF": ("Generar la factura PDF", "Generate the PDF invoice"),
    "Partager la facture": ("Compartir la factura", "Share the invoice"),
    "FACTURE": ("FACTURA", "INVOICE"),
    "N° %@ · %@": ("N.º %@ · %@", "No. %@ · %@"),
    "SIRET %@": ("NIF %@", "Company ID %@"),
    "Facturé à :": ("Facturado a:", "Billed to:"),
    "Date": ("Fecha", "Date"),
    "Trajet": ("Trayecto", "Trip"),
    "Total : %.1f km × %.2f €/km = %.2f €": (
        "Total: %.1f km × %.2f €/km = %.2f €",
        "Total: %.1f km × %.2f €/km = %.2f €",
    ),
    "Trace GPX 🥾": ("Traza GPX 🥾", "GPX track 🥾"),
    "Importer une trace GPX": ("Importar una traza GPX", "Import a GPX track"),
    "Une randonnée, un parcours vélo exporté d'une autre app : la trace s'affiche sur la carte et se laisse suivre pas à pas, même sans réseau.": (
        "Una caminata, una ruta en bici exportada de otra app: la traza se muestra en el mapa y se puede seguir paso a paso, incluso sin red.",
        "A hike, a bike route exported from another app: the track shows on the map and can be followed step by step, even without network.",
    ),
    "Ce fichier GPX n'a pas pu être lu.": (
        "Este archivo GPX no se pudo leer.",
        "This GPX file could not be read.",
    ),
    "Temps d'arrêt": ("Tiempo de parada", "Stop time"),
    "Sans arrêt": ("Sin parada", "No stop"),
    "Étape : %@": ("Parada: %@", "Stop: %@"),
    "🔥 Traverse une zone de feux vers le km %d": (
        "🔥 Atraviesa una zona de incendios hacia el km %d",
        "🔥 Crosses a wildfire area around km %d",
    ),
    "🫁 Air %@ à l'arrivée": ("🫁 Aire %@ a la llegada", "🫁 %@ air at arrival"),
    "🌧️ Pluie vers %@ (%@ · %d %%)": (
        "🌧️ Lluvia hacia %@ (%@ · %d %%)",
        "🌧️ Rain near %@ (%@ · %d %%)",
    ),
    "🌧️ Pluie en route (%@ · %d %%)": (
        "🌧️ Lluvia en el camino (%@ · %d %%)",
        "🌧️ Rain on the way (%@ · %d %%)",
    ),
    "🌧️ Pluie probable à l'arrivée (%d %%)": (
        "🌧️ Lluvia probable a la llegada (%d %%)",
        "🌧️ Rain likely at arrival (%d %%)",
    ),
    "Transports": ("Transporte", "Transit"),
    "Maintenant": ("Ahora", "Now"),
    "il y a %d h": ("hace %d h", "%d h ago"),
    "Partir à": ("Salir a las", "Leave at"),
    "Arriver avant": ("Llegar antes de las", "Arrive by"),
    "%.0f min · direct · %@": ("%.0f min · directo · %@", "%.0f min · direct · %@"),
    "%.0f min · 1 correspondance · %@": (
        "%.0f min · 1 transbordo · %@",
        "%.0f min · 1 transfer · %@",
    ),
    "%.0f min · %d correspondances · %@": (
        "%.0f min · %d transbordos · %@",
        "%.0f min · %d transfers · %@",
    ),
    "Les transports en commun ont besoin du réseau.": (
        "El transporte público necesita cobertura.",
        "Public transit needs a network connection.",
    ),
    "Aucun trajet en transports trouvé pour cette destination.": (
        "No se ha encontrado ningún trayecto en transporte para este destino.",
        "No transit journey found for this destination.",
    ),
    "%@ %@ direction %@ — descendre à %@": (
        "%@ %@ dirección %@ — bajar en %@",
        "%@ %@ towards %@ — get off at %@",
    ),
    "%@ direction %@ — descendre à %@": (
        "%@ dirección %@ — bajar en %@",
        "%@ towards %@ — get off at %@",
    ),
    "Marcher jusqu'à %@": ("Caminar hasta %@", "Walk to %@"),
    "la destination": ("el destino", "the destination"),
    "Tram": ("Tranvía", "Tram"),
    "Bus": ("Bus", "Bus"),
    "Métro": ("Metro", "Subway"),
    "Train": ("Tren", "Train"),
    "Bac": ("Ferri", "Ferry"),
    "Marche": ("A pie", "Walking"),
    "Aucune région d'itinéraires n'est installée. Télécharge celle qui te concerne depuis l'écran hors ligne.": (
        "No hay ninguna región de rutas instalada. Descarga la que te interesa desde la pantalla sin conexión.",
        "No routing region is installed. Download the one you need from the offline screen.",
    ),
    "Le paquet « %@ » ne s'ouvre pas. Supprime-le et réinstalle-le depuis l'écran hors ligne.": (
        "El paquete «%@» no se abre. Elimínalo y vuelve a instalarlo desde la pantalla sin conexión.",
        "The “%@” package will not open. Delete it and reinstall it from the offline screen.",
    ),
    "« %@ » ne couvre pas ce trajet. Il doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.": (
        "«%@» no cubre este trayecto. Debe caber entero en un mismo paquete: para eso sirven las zonas, que se solapan.",
        "“%@” does not cover this trip. It must fit entirely within one package — that is what the overlapping areas are for.",
    ),
    "« %@ » a refusé le calcul : %@": (
        "«%@» ha rechazado el cálculo: %@",
        "“%@” refused the computation: %@",
    ),
    "« %@ » n'a trouvé aucune route entre ces deux points, sans dire pourquoi.": (
        "«%@» no ha encontrado ninguna ruta entre estos dos puntos, sin decir por qué.",
        "“%@” found no route between these two points, without saying why.",
    ),
    "Aucune région installée ne couvre ce trajet. Il doit tenir entier dans un même paquet : c'est à cela que servent les zones, qui se chevauchent.": (
        "Ninguna región instalada cubre este trayecto. Debe caber entero en un mismo paquete: para eso sirven las zonas, que se solapan.",
        "No installed region covers this trip. It must fit entirely within one package — that is what the overlapping areas are for.",
    ),
    "Qualité de l'air": ("Calidad del aire", "Air quality"),
    "Indice européen": ("Índice europeo", "European index"),
    "Indice américain": ("Índice estadounidense", "US index"),
    "Bon": ("Bueno", "Good"),
    "Correct": ("Aceptable", "Fair"),
    "Dégradé": ("Moderado", "Moderate"),
    "Mauvais": ("Malo", "Poor"),
    "Très mauvais": ("Muy malo", "Very poor"),
    "Extrêmement mauvais": ("Extremadamente malo", "Extremely poor"),
    "Particules fines : %.0f µg/m³": (
        "Partículas finas: %.0f µg/m³",
        "Fine particles: %.0f µg/m³",
    ),
    "Mesures Open-Meteo (CAMS)": ("Datos Open-Meteo (CAMS)", "Open-Meteo data (CAMS)"),
    "Autour de toi": ("A tu alrededor", "Around you"),
    "Mesures momentanément indisponibles.": (
        "Mediciones no disponibles por ahora.",
        "Measurements unavailable right now.",
    ),
    "Point touché": ("Punto seleccionado", "Selected point"),
    "Voir le monde entier": ("Ver el mundo entero", "See the whole world"),
    "Recentrer sur ma position": (
        "Centrar en mi posición",
        "Recenter on my location",
    ),
    "Aucune mesure ici. Rapproche-toi des terres.": (
        "Ninguna medición aquí. Acércate a tierra firme.",
        "No measurement here. Move closer to land.",
    ),
    "Mesures indisponibles — il faut du réseau.": (
        "Mediciones no disponibles: hace falta cobertura.",
        "Measurements unavailable — a network connection is needed.",
    ),
    "Mise à jour il y a moins d'1 h": (
        "Actualizado hace menos de 1 h",
        "Updated less than 1 h ago",
    ),
    "Mise à jour il y a %lld h": ("Actualizado hace %lld h", "Updated %lld h ago"),
    "%lld point chaud": ("%lld punto caliente", "%lld hot spot"),
    "%lld points chauds": ("%lld puntos calientes", "%lld hot spots"),
    "détections satellite · %@": (
        "detecciones por satélite · %@",
        "satellite detections · %@",
    ),
    "Chercher une région": ("Buscar una región", "Search a region"),
    "mot de passe": ("contraseña", "password"),
    "e-mail": ("correo electrónico", "email"),
    "Adresse e-mail invalide.": ("Correo electrónico no válido.", "Invalid email address."),
    "E-mail ou mot de passe incorrect.": (
        "Correo o contraseña incorrectos.",
        "Wrong email or password.",
    ),
    "Ton e-mail sert d'identifiant — il ne sert qu'à ça, et à retrouver ton mot de passe.": (
        "Tu correo sirve de identificador — solo para eso, y para recuperar tu contraseña.",
        "Your email is your login — used only for that, and to recover your password.",
    ),
    "Mot de passe oublié ?": ("¿Contraseña olvidada?", "Forgot your password?"),
    "Si un compte existe avec cette adresse, un lien vient d'être envoyé. Regarde ta boîte (et les indésirables).": (
        "Si existe una cuenta con esa dirección, se acaba de enviar un enlace. Mira tu buzón (y el correo no deseado).",
        "If an account exists for that address, a link was just sent. Check your inbox (and spam).",
    ),
    "La réinitialisation n'est pas encore configurée.": (
        "La recuperación aún no está configurada.",
        "Password recovery isn't configured yet.",
    ),
    "Impossible de créer le compte.": (
        "No se pudo crear la cuenta.",
        "Couldn't create the account.",
    ),
    "Serveur injoignable. Réessaie.": (
        "Servidor inalcanzable. Reintenta.",
        "Server unreachable. Try again.",
    ),
    "Échec de l'authentification": ("Fallo de autenticación", "Authentication failed"),
    "Saisie invalide.": ("Entrada no válida.", "Invalid input."),
    # Messages renvoyés par le serveur, affichés tels quels par l'écran de
    # connexion (LocalizedStringKey) : présents ici, ils se traduisent.
    "nom d'utilisateur déjà pris": (
        "nombre de usuario ya en uso",
        "username already taken",
    ),
    "nom (≥3) et mot de passe (≥6) requis": (
        "nombre (≥3) y contraseña (≥6) requeridos",
        "name (≥3) and password (≥6) required",
    ),
    "Clé API TomTom": ("Clave de API TomTom", "TomTom API key"),
    "Clé FIRMS (MAP_KEY)": ("Clave FIRMS (MAP_KEY)", "FIRMS key (MAP_KEY)"),
    "Type de vélo": ("Tipo de bicicleta", "Bike type"),
    "Route": ("Carretera", "Road"),
    "Ville": ("Ciudad", "City"),
    "VTT": ("BTT", "MTB"),
    "Éviter les côtes": ("Evitar las cuestas", "Avoid hills"),
    "Éviter les péages": ("Evitar peajes", "Avoid tolls"),
    "Éviter les autoroutes": ("Evitar autopistas", "Avoid motorways"),
    "Un trajet vers chaque point": ("Una ruta hasta cada punto", "One route to each point"),
    "Itinéraire le plus simple": ("La ruta más sencilla", "The simplest route"),
    "Gazole": ("Gasóleo", "Diesel"),
    # --- Phrases et explications ---
    "Reprendre la navigation ?": ("¿Reanudar la navegación?", "Resume navigation?"),
    "Partage impossible": ("No se puede compartir", "Sharing failed"),
    "Messages indisponible": ("Mensajes no disponible", "Messages unavailable"),
    "Aucune région de ce nom.": ("Ninguna región con ese nombre.", "No region by that name."),
    "Contour satellite approximatif": ("Contorno aproximado por satélite", "Approximate satellite outline"),
    "Aucune position — utilise « Enregistrer ma position ».": (
        "Ninguna posición — usa «Guardar mi posición».",
        "No position — use “Save my position”.",
    ),
    "Aucun trajet enregistré pour l'instant. Lance une navigation : ton trajet sera sauvegardé automatiquement à l'arrivée.": (
        "Todavía no hay ningún viaje guardado. Inicia una navegación: tu viaje se guardará solo al llegar.",
        "No trip saved yet. Start navigating: your trip is saved automatically on arrival.",
    ),
    "Tape une adresse ou un lieu, puis choisis une destination.": (
        "Escribe una dirección o un lugar y elige un destino.",
        "Type an address or a place, then pick a destination.",
    ),
    "Coche des points pour tracer plusieurs trajets à la fois.": (
        "Marca puntos para trazar varias rutas a la vez.",
        "Tick points to draw several trips at once.",
    ),
    "Liste des régions indisponible — il faut du réseau une fois.": (
        "Lista de regiones no disponible: hace falta cobertura una vez.",
        "Region list unavailable — signal is needed once.",
    ),
    "Itinéraire trop long à ce niveau de détail (%@). Choisis « Région ».": (
        "Ruta demasiado larga con este nivel de detalle (%@). Elige «Región».",
        "Route too long at this level of detail (%@). Choose “Region”.",
    ),
    "Zone trop vaste : %@ environ. Rapproche-toi, ou choisis un niveau de détail moindre.": (
        "Zona demasiado amplia: %@ aproximadamente. Acércate o elige un nivel de detalle menor.",
        "Area too large: about %@. Move closer, or choose a lower level of detail.",
    ),
    "Cet appareil ne peut pas envoyer de SMS. Copie ton message et envoie-le autrement.": (
        "Este dispositivo no puede enviar SMS. Copia tu mensaje y envíalo de otro modo.",
        "This device cannot send texts. Copy your message and send it another way.",
    ),
    "Laisse vide si tu n'héberges rien. Une adresse qui ne répond pas est ignorée : la carte reste affichée avec le fond par défaut.": (
        "Déjalo vacío si no alojas nada. Una dirección que no responde se ignora: el mapa sigue mostrándose con el fondo por defecto.",
        "Leave empty if you host nothing. An address that doesn't answer is ignored: the map stays up with the default background.",
    ),
    "Colle ta clé TomTom pour activer les vrais bouchons en temps réel : la navigation contournera automatiquement les embouteillages sur ta route.": (
        "Pega tu clave TomTom para activar los atascos reales en tiempo real: la navegación esquivará sola las retenciones de tu ruta.",
        "Paste your TomTom key to switch on real live jams: navigation will route around the congestion ahead of you.",
    ),
    "Reçois une notification quand un signalement (police, accident, danger…) se trouve à moins de 600 m, même écran verrouillé.": (
        "Recibe un aviso cuando un reporte (policía, accidente, peligro…) esté a menos de 600 m, incluso con la pantalla bloqueada.",
        "Get a notification when a report (police, crash, hazard…) is within 600 m, even with the screen locked.",
    ),
    "Supprimer définitivement ton compte et toutes tes données (positions, trajets, recherches, signalements) ?": (
        "¿Eliminar definitivamente tu cuenta y todos tus datos (posiciones, viajes, búsquedas, avisos)?",
        "Permanently delete your account and all your data (positions, trips, searches, reports)?",
    ),
    # --- Chaînes interpolées ---------------------------------------------
    # SwiftUI transforme `Text("Mes trajets (\(n))")` en clé « Mes trajets
    # (%lld) ». Un spécificateur mal deviné ne casse rien : la traduction ne
    # s'applique pas et le français reste.
    "Mes trajets (%lld)": ("Mis viajes (%lld)", "My trips (%lld)"),
    "Afficher les trajets cochés (%lld)": (
        "Mostrar los viajes marcados (%lld)",
        "Show the ticked trips (%lld)",
    ),
    "Supprimer les points cochés (%lld)": (
        "Eliminar los puntos marcados (%lld)",
        "Delete the ticked points (%lld)",
    ),
    "Définir %@ sur ma position": ("Fijar %@ en mi posición", "Set %@ to my position"),
    "Confirmé %lld fois · infirmé %lld fois": (
        "Confirmado %lld veces · desmentido %lld veces",
        "Confirmed %lld times · denied %lld times",
    ),
    "%lld trajets affichés": ("%lld viajes mostrados", "%lld trips shown"),
    "%lld signalements · %lld trajets": (
        "%lld avisos · %lld viajes",
        "%lld reports · %lld trips",
    ),
    "%lld points · %@": ("%lld puntos · %@", "%lld points · %@"),
    "%lld régions · %@": ("%lld regiones · %@", "%lld regions · %@"),
    "Mon itinéraire · %@": ("Mi ruta · %@", "My route · %@"),
    "%lld bouchon(s) détecté(s) autour de toi (temps réel).": (
        "%lld atasco(s) detectado(s) a tu alrededor (tiempo real).",
        "%lld jam(s) detected around you (live).",
    ),
    # --- Chaînes de `String(format:)` -------------------------------------
    # Enveloppées dans `NSLocalizedString` côté Swift : sans cela,
    # `String(format:)` ne consulte aucun catalogue.
    "%.0f min · %.1f km · %d virages · arr. %@": (
        "%.0f min · %.1f km · %d giros · lleg. %@",
        "%.0f min · %.1f km · %d turns · arr. %@",
    ),
    "%.1f km restants": ("%.1f km restantes", "%.1f km left"),
    "Itinéraire de %.1f km en cours.": (
        "Ruta de %.1f km en curso.",
        "Route of %.1f km in progress.",
    ),
    "↗ %.0f m de côte · ↘ %.0f m de descente": (
        "↗ %.0f m de subida · ↘ %.0f m de bajada",
        "↗ %.0f m of climb · ↘ %.0f m of descent",
    ),
    "⚠️ Forte côte (%.0f m)": ("⚠️ Subida fuerte (%.0f m)", "⚠️ Steep climb (%.0f m)"),
    "⚠️ Forte descente (%.0f m)": ("⚠️ Bajada fuerte (%.0f m)", "⚠️ Steep descent (%.0f m)"),
    "⚠️ Forte côte (%.0f m) et forte descente (%.0f m)": (
        "⚠️ Subida fuerte (%.0f m) y bajada fuerte (%.0f m)",
        "⚠️ Steep climb (%.0f m) and steep descent (%.0f m)",
    ),
    "⛽ ~%.2f € · sans péage": ("⛽ ~%.2f € · sin peaje", "⛽ ~%.2f € · no tolls"),
    "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €": (
        "⛽ ~%.2f € · 🛣️ peajes ~%.2f € · total ~%.2f €",
        "⛽ ~%.2f € · 🛣️ tolls ~%.2f € · total ~%.2f €",
    ),
    # --- Signalements et niveaux de détail --------------------------------
    # Les libellés de signalement circulent en français sur le réseau : chaque
    # appareil les traduit à l'affichage. Sans cela, un signalement posté
    # depuis un téléphone espagnol s'afficherait en espagnol chez tous.
    "Police": ("Policía", "Police"),
    "Accident": ("Accidente", "Crash"),
    "Bouchon": ("Atasco", "Jam"),
    "Danger": ("Peligro", "Hazard"),
    "Véhicule arrêté": ("Vehículo parado", "Stopped vehicle"),
    "Objet sur la route": ("Objeto en la vía", "Object on the road"),
    "Travaux": ("Obras", "Roadworks"),
    # Niveaux de détail du téléchargement de carte.
    "Région": ("Región", "Region"),
    "Ville": ("Ciudad", "City"),
    "VTT": ("BTT", "MTB"),
    "Rue": ("Calle", "Street"),
    # --- Consignes de navigation ------------------------------------------
    # L'application ne montre pas la consigne du moteur d'itinéraire : elle la
    # ré-écrit en un mot, lisible d'un coup d'œil au volant. Ces textes sont
    # donc les siens, et doivent être traduits comme le reste.
    "Calcul de l'itinéraire…": ("Calculando la ruta…", "Computing the route…"),
    "calcul de l'itinéraire": ("calculando la ruta", "computing the route"),
    "Recalcul de l'itinéraire…": ("Recalculando la ruta…", "Recomputing the route…"),
    "Recalcul de l'itinéraire.": ("Recalculando la ruta.", "Recomputing the route."),
    "Arrivée": ("Llegada", "Arrival"),
    "vous êtes arrivé": ("has llegado", "you have arrived"),
    "Demi-tour": ("Cambio de sentido", "U-turn"),
    "faites demi-tour": ("haz un cambio de sentido", "make a U-turn"),
    "Gauche": ("Izquierda", "Left"),
    "Droite": ("Derecha", "Right"),
    "à gauche": ("a la izquierda", "left"),
    "à droite": ("a la derecha", "right"),
    "Tout droit": ("Recto", "Straight ahead"),
    "tout droit": ("recto", "straight ahead"),
    "rond-point": ("rotonda", "roundabout"),
    "serrez": ("mantente", "keep"),
    "virage serré": ("giro cerrado", "sharp turn"),
    "serrez %@": ("mantente %@", "keep %@"),
    "franchement %@": ("cerrado %@", "sharp %@"),
    "sortie": ("salida", "exit"),
    "Sortie": ("Salida", "Exit"),
    "Sortie %@": ("Salida %@", "Exit %@"),
    "%@ sortie": ("%@ salida", "%@ exit"),
    "prenez la sortie": ("toma la salida", "take the exit"),
    "prenez la sortie %@": ("toma la salida %@", "take exit %@"),
    "prenez la %@ sortie": ("toma la %@ salida", "take the %@ exit"),
    "au rond-point, %@": ("en la rotonda, %@", "at the roundabout, %@"),
    "au rond-point, prenez la %@ sortie": (
        "en la rotonda, toma la %@ salida",
        "at the roundabout, take the %@ exit",
    ),
    # Rangs de sortie. Une table plutôt qu'un `NumberFormatter` : celui-ci
    # donne « 1er » là où une sortie, mot féminin, demande « 1re ».
    "1re": ("1.ª", "1st"),
    "2e": ("2.ª", "2nd"),
    "3e": ("3.ª", "3rd"),
    "4e": ("4.ª", "4th"),
    "5e": ("5.ª", "5th"),
    "6e": ("6.ª", "6th"),
    "7e": ("7.ª", "7th"),
    "8e": ("8.ª", "8th"),
    "9e": ("9.ª", "9th"),
    # --- Annonces vocales ---------------------------------------------------
    "Dans %d mètres, %@": ("En %d metros, %@", "In %d metres, %@"),
    "Maintenant, %@": ("Ahora, %@", "Now, %@"),
    "Départ, %@": ("Salida, %@", "Starting, %@"),
    "Arrivée prévue vers %@.": ("Llegada prevista hacia las %@.", "Arriving around %@."),
    "Vous êtes arrivé à destination.": (
        "Has llegado a tu destino.",
        "You have arrived at your destination.",
    ),
    "Nouvel itinéraire.": ("Nueva ruta.", "New route."),
    "Itinéraire plus rapide trouvé : environ %d minutes de gagnées.": (
        "Ruta más rápida encontrada: unos %d minutos ganados.",
        "Faster route found: about %d minutes saved.",
    ),
    "Ça bouchonne devant. Itinéraire de contournement.": (
        "Hay atasco delante. Ruta alternativa.",
        "Traffic ahead. Taking a way around.",
    ),
    "Bouchon signalé devant. Itinéraire pour l'éviter.": (
        "Atasco avisado delante. Ruta para evitarlo.",
        "Jam reported ahead. Routing around it.",
    ),
    "Recalcul impossible. Je garde l'itinéraire.": (
        "No se puede recalcular. Mantengo la ruta.",
        "Cannot recompute. Keeping the route.",
    ),
    # --- Catégories de lieux et modes de déplacement -----------------------
    # Ces libellés viennent d'énumérations, pas de littéraux : les sites
    # d'appel passent par `LocalizedStringKey` pour que la recherche ait lieu.
    "Essence": ("Gasolinera", "Fuel"),
    "Parking": ("Aparcamiento", "Parking"),
    "Handicapé": ("Accesible", "Accessible"),
    "Recharge": ("Recarga", "Charging"),
    "Toilettes": ("Aseos", "Toilets"),
    "Baignade": ("Baño", "Swimming"),
    "Pharmacie": ("Farmacia", "Pharmacy"),
    "Distributeur": ("Cajero", "ATM"),
    "Supermarché": ("Supermercado", "Supermarket"),
    "Camping": ("Camping", "Campsite"),
    "Eau potable": ("Agua potable", "Drinking water"),
    "Point de vue": ("Mirador", "Viewpoint"),
    "Fast-food": ("Comida rápida", "Fast food"),
    "Restaurants": ("Restaurantes", "Restaurants"),
    "Hôtels": ("Hoteles", "Hotels"),
    "Tourisme": ("Turismo", "Sightseeing"),
    "Sorties": ("Ocio", "Nightlife"),
    # Conditions météo (Weather.describe) : affichées par `Text(String)`.
    "Ensoleillé": ("Soleado", "Sunny"),
    "Éclaircies": ("Claros", "Partly cloudy"),
    "Couvert": ("Cubierto", "Overcast"),
    "Brouillard": ("Niebla", "Fog"),
    "Bruine": ("Llovizna", "Drizzle"),
    "Pluie": ("Lluvia", "Rain"),
    "Neige": ("Nieve", "Snow"),
    "Averses": ("Chubascos", "Showers"),
    "Orage": ("Tormenta", "Thunderstorm"),
    "Météo": ("Tiempo", "Weather"),
    # Étiquettes du comparateur : construites dans une structure, donc
    # passées par `LocalizedStringKey` à l'affichage.
    "Le plus simple": ("La más sencilla", "The simplest"),
    "Le plus rapide": ("La más rápida", "The fastest"),
    "Le plus court": ("La más corta", "The shortest"),
    "Alternative": ("Alternativa", "Alternative"),
    "Itinéraire": ("Ruta", "Route"),
    # Messages de validation : affichés par `Text(String)`, donc passés par
    # `LocalizedStringKey` au site d'appel.
    "Mot de passe : 6 caractères minimum.": (
        "Contraseña: 6 caracteres como mínimo.",
        "Password: 6 characters minimum.",
    ),
    "À pied": ("A pie", "On foot"),
    "Vélo": ("Bici", "Bike"),
    "Voiture": ("Coche", "Car"),
}


# Les libellés que la vitrine reproduit dans ses maquettes. Les exporter
# depuis ce tableau plutôt que les retaper garantit qu'un écran dessiné sur
# le site dit exactement ce que l'application affiche.
VITRINE = [
    "À pied", "Vélo", "Voiture", "Transports", "Démarrer",
    "Le plus simple", "Le plus rapide", "Le plus court",
    "Essence", "Parking", "Recharge", "Restaurants", "Hôtels", "Supermarché",
    "Pharmacie", "Distributeur", "Toilettes", "Eau potable", "Handicapé",
    "Baignade", "Surf", "Skatepark", "Camping", "Point de vue", "Fast-food",
    "Tourisme", "Sorties", "Éclaircies",
    # Le bandeau de manœuvre : un mot et sa précision, pas une phrase.
    "Sortie %@", "sortie",
    # Les formats de `String(format:)` : la vitrine les remplit avec des
    # valeurs d'exemple plutôt que de recopier des phrases toutes faites.
    "%.0f min · %.1f km · %d virages · arr. %@",
    "↗ %.0f m de côte · ↘ %.0f m de descente",
    "⚠️ Forte côte (%.0f m)",
    "⛽ ~%.2f € · sans péage",
    "⛽ ~%.2f € · 🛣️ péages ~%.2f € · total ~%.2f €",
]


def exporte_vitrine(chemin: pathlib.Path) -> None:
    """Écrit le module TypeScript que la vitrine importe pour ses maquettes."""
    lignes = [
        "// Généré par frontend/traductions.py — ne pas modifier à la main.",
        "//",
        "// Les libellés que les maquettes reproduisent, dans les trois langues,",
        "// tirés du même tableau que le catalogue de l'application. Un écran",
        "// dessiné sur le site dit ainsi exactement ce que l'app affiche.",
        "",
        'export type LangueApp = "fr" | "es" | "en";',
        "",
        "export const APP: Record<LangueApp, Record<string, string>> = {",
    ]
    for langue in ("fr", "es", "en"):
        lignes.append(f"  {langue}: {{")
        for cle in VITRINE:
            # Absent du tableau : le mot ne change pas d'une langue à
            # l'autre (« Surf », « Skatepark »). Retomber sur la clé, c'est
            # exactement ce que fait l'application.
            es, en = T.get(cle, (cle, cle))
            valeur = {"fr": cle, "es": es, "en": en}[langue]
            lignes.append(f'    {json.dumps(cle, ensure_ascii=False)}: '
                          f'{json.dumps(valeur, ensure_ascii=False)},')
        lignes.append("  },")
    lignes += ["};", ""]
    chemin.write_text("\n".join(lignes), encoding="utf-8")
    print(f"{chemin.name} : {len(VITRINE)} libellés × 3 langues")


# --- Contrôles ------------------------------------------------------------
#
# Chacun vient d'une panne réelle. Ils tournent en intégration continue parce
# qu'aucune ne fait échouer la compilation : une chaîne non traduite s'affiche
# simplement en français, et rien ne proteste.

def sources() -> list[tuple[str, str]]:
    dossier = pathlib.Path(__file__).parent / "MonCapGPS"
    return [(str(f), f.read_text(encoding="utf-8")) for f in sorted(dossier.glob("*.swift"))]


def concatenations(fichiers: list[tuple[str, str]]) -> list[str]:
    """Repère `Text("a" + "b")`, qui ne se traduit jamais.

    La concaténation produit une `String`, et cette surcharge est verbatim :
    le catalogue n'est pas consulté. Quatre explications de réglages sont
    restées ainsi en français alors que leurs clés existaient bel et bien.
    """
    motif = re.compile(r'(?:Text|Label|Button)\(\s*\n?\s*"(?:[^"\\]|\\.)*"\s*\n?\s*\+')
    trouves = []
    for nom, texte in fichiers:
        for m in motif.finditer(texte):
            trouves.append(f"{nom}:{texte[:m.start()].count(chr(10)) + 1}")
    return trouves


def verbatim(fichiers: list[tuple[str, str]]) -> list[str]:
    """Repère `Text(uneVariable)` sans `LocalizedStringKey`.

    Cette surcharge est verbatim elle aussi. Les libellés venus
    d'énumérations — catégories de lieux, modes, météo — y passaient tous à
    côté du catalogue.
    """
    # Une variable, pas un littéral : pas de guillemet après la parenthèse.
    motif = re.compile(r'\bText\(\s*([a-z]\w*(?:\.\w+)+)\s*\)')
    # Ce qui n'a rien à traduire : de la donnée d'utilisateur (noms de lieux,
    # de trajets, de contributeurs), des emoji, des tailles déjà formatées, et
    # les consignes que `NavigationManager` a déjà traduites à la source.
    permis = {
        "nav.instruction", "router.message", "packs.message",
        "kind.emoji", "w.emoji", "t.emoji",
        "gardee.poids", "region.nom", "couvrante.nom",
        "r.label", "r.name", "r.subtitle",
        "place.label", "p.label", "e.name", "t.label",
        # Déjà typés LocalizedStringKey à la source (PlusAtout, MonCapPlus) :
        # la traduction se fait bien, le contrôleur ne voit que la surface.
        "a.titre", "a.detail",
    }
    # Limite assumée : la liste est indexée par expression, non par site.
    # « t.label » désigne ici le nom d'un trajet — donnée d'utilisateur — mais
    # aussi, ailleurs dans le même fichier, un libellé de signalement qui, lui,
    # se traduit. Un retour en arrière à cet endroit passerait donc au travers.
    trouves = []
    for nom, texte in fichiers:
        for m in motif.finditer(texte):
            expr = m.group(1)
            if expr in permis:
                continue
            trouves.append(f"{nom}:{texte[:m.start()].count(chr(10)) + 1} → Text({expr})")
    return trouves


def cles_orphelines(fichiers: list[tuple[str, str]]) -> list[str]:
    """Clés du catalogue qui ne correspondent plus à rien dans le code."""
    tout = "\n".join(t for _, t in fichiers)
    # Les clés à spécificateur viennent de l'interpolation SwiftUI ou d'un
    # format : elles n'apparaissent pas telles quelles dans les sources.
    rangs = {"1re"} | {f"{n}e" for n in range(2, 10)}
    # Les messages venus du serveur ne figurent dans aucun source Swift :
    # ils arrivent par le réseau, et l'écran de connexion les affiche en
    # LocalizedStringKey — présents au catalogue, ils se traduisent.
    serveur = {
        "nom d'utilisateur déjà pris",
        "nom (≥3) et mot de passe (≥6) requis",
    }
    return [
        k for k in T
        if "%" not in k and k not in rangs and k not in serveur
        and f'"{k}"' not in tout
    ]


def appels_sans_cle(fichiers: list[tuple[str, str]]) -> list[str]:
    """Appels `tr`/`trf` dont la clé manque au tableau."""
    trouves = []
    for nom, texte in fichiers:
        for i, ligne in enumerate(texte.splitlines(), 1):
            if ligne.lstrip().startswith("//"):
                continue
            for cle in re.findall(r'\btrf?\("((?:[^"\\]|\\.)*)"', ligne):
                if cle not in T:
                    trouves.append(f"{nom}:{i} → « {cle} »")
    return trouves


def verifie() -> int:
    fichiers = sources()
    controles = [
        ("concaténations qui neutralisent la traduction", concatenations(fichiers)),
        ("Text(variable) sans LocalizedStringKey", verbatim(fichiers)),
        ("clés du catalogue devenues orphelines", cles_orphelines(fichiers)),
        ("appels tr/trf sans clé", appels_sans_cle(fichiers)),
    ]
    fautes = 0
    for titre, trouves in controles:
        if trouves:
            fautes += len(trouves)
            print(f"::error::{titre} ({len(trouves)}) :")
            for t in trouves:
                print(f"  {t}")
        else:
            print(f"{titre} : aucune ✅")
    return 1 if fautes else 0


def main() -> None:
    if "--verifie" in sys.argv:
        raise SystemExit(verifie())

    strings = {
        cle: {
            "localizations": {
                langue: {"stringUnit": {"state": "translated", "value": valeur}}
                for langue, valeur in (("en", en), ("es", es))
            }
        }
        for cle, (es, en) in T.items()
    }
    catalogue = {
        "sourceLanguage": "fr",
        "strings": dict(sorted(strings.items())),
        "version": "1.0",
    }
    chemin = pathlib.Path(__file__).parent / "MonCapGPS" / "Localizable.xcstrings"
    chemin.write_text(
        json.dumps(catalogue, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"{chemin.name} : {len(T)} clés, 2 langues")

    exporte_vitrine(pathlib.Path(__file__).parent.parent / "vitrine" / "src" / "appstrings.ts")


if __name__ == "__main__":
    main()

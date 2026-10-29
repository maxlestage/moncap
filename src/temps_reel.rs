//! Le temps réel : le WebSocket `/ws` et la diffusion des événements.
//!
//! Chaque événement part à tous les clients connectés. Le serveur tourne en
//! une seule instance (sa base est un fichier) : le canal de diffusion est
//! partagé par tous ses fils, et il n'y a plus d'autres instances à
//! prévenir — le pont Redis d'avant n'a plus lieu d'être. Les messages sont
//! du JSON étiqueté par `"kind"` (snake_case), comme avant.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use paquets::prelude::*;
use paquets::vitesse::ws::{self, Message};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::alertes::{self, Alerte};
use crate::commun::{coord_valide, nettoie_label};
use crate::connexion;
use crate::erreur::{Erreur, Resultat};

/// Le canal de diffusion de l'instance, rangé dans l'état de l'application.
pub struct Hub {
    tx: broadcast::Sender<String>,
    /// Préfixe aléatoire des identifiants de connexion : un client resté
    /// connecté à travers un redémarrage ne retrouve pas l'identifiant d'un
    /// autre.
    prefixe: u64,
    suivant: AtomicU64,
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        // 20 bits aléatoires au-dessus de 32 bits de compteur : on reste sous
        // 2^53, l'entier exact le plus grand d'un nombre JavaScript.
        let prefixe = (OsRng.next_u64() & 0xF_FFFF) << 32;
        Hub {
            tx,
            prefixe,
            suivant: AtomicU64::new(1),
        }
    }

    fn nouvel_id(&self) -> u64 {
        self.prefixe | (self.suivant.fetch_add(1, Ordering::Relaxed) & 0xFFFF_FFFF)
    }

    /// Diffuse un événement à tous les clients connectés.
    pub fn diffuse(&self, ev: &Evenement) {
        if let Ok(json) = serde_json::to_string(ev) {
            let _ = self.tx.send(json);
        }
    }

    pub fn abonne(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

/// Le hub, rangé dans l'état de l'application.
pub struct HubRef(pub &'static Hub);

/// Le hub de l'application.
pub fn hub(req: &Request) -> &'static Hub {
    req.state::<HubRef>().0
}

/// Message diffusé aux clients.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evenement {
    /// L'identifiant de cette connexion, envoyé juste après l'ouverture : le
    /// client s'en sert pour écarter l'écho de sa propre position live.
    Hello { id: u64 },
    /// Les positions enregistrées ont changé : le client recharge.
    PositionsChanged,
    /// Position live d'un utilisateur.
    Live {
        id: u64,
        lat: f64,
        lon: f64,
        label: String,
        avatar: String,
    },
    /// Un utilisateur live s'est déconnecté.
    LiveGone { id: u64 },
    /// Un signalement nouveau ou mis à jour (votes).
    Alert(Alerte),
    /// Un signalement supprimé.
    AlertGone { id: i64 },
    /// Instantané des signalements en cours (à la connexion).
    Alerts { alerts: Vec<Alerte> },
}

/// Message reçu d'un client.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MessageClient {
    Live {
        lat: f64,
        lon: f64,
        label: String,
        #[serde(default)]
        avatar: String,
    },
    Alert {
        category: String,
        lat: f64,
        lon: f64,
        #[serde(default)]
        label: String,
    },
}

// ---------------------------------------------------------------------------
// Le WebSocket.
// ---------------------------------------------------------------------------

pub fn routes(app: &mut App) {
    app.get("/ws", connexion_ws);
}

/// Toutes les minutes, la session du socket est revérifiée : une
/// déconnexion ou une suppression de compte ferme aussi le temps réel.
const REVERIFICATION: Duration = Duration::from_secs(60);

/// GET /ws?token=… — le navigateur ne peut pas poser d'en-tête
/// `Authorization` sur un WebSocket : le jeton passe dans l'URL.
async fn connexion_ws(req: Request) -> Resultat<Response> {
    let jeton = req
        .query("token")
        .map(|t| t.into_owned())
        .ok_or_else(|| Erreur::requete("jeton manquant"))?;
    let qui = connexion::depuis_jeton(req.db(), &jeton).await?;
    let montee = ws::Upgrade::new(&req)?.max_message_size(64 * 1024);
    Ok(montee.on_upgrade(req, move |req, socket| async move {
        session_ws(req, socket, jeton, qui.user_id).await;
    }))
}

async fn envoie(socket: &mut ws::WebSocket, ev: &Evenement) -> bool {
    match serde_json::to_string(ev) {
        Ok(json) => socket.send(json).await.is_ok(),
        Err(_) => true,
    }
}

async fn session_ws(req: Request, mut socket: ws::WebSocket, jeton: String, uid: i32) {
    let hub = hub(&req);
    let db = req.db();
    let id = hub.nouvel_id();
    let mut rx = hub.abonne();

    if envoie(&mut socket, &Evenement::Hello { id }).await {
        let alerts = alertes::actives(db).await.unwrap_or_default();
        let _ = envoie(&mut socket, &Evenement::Alerts { alerts }).await;
    }

    let mut controle = tokio::time::interval(REVERIFICATION);
    controle.tick().await;
    loop {
        tokio::select! {
            recu = rx.recv() => match recu {
                Ok(msg) => {
                    if socket.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(t))) => message_client(hub, db, id, uid, &t).await,
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            _ = controle.tick() => {
                if matches!(
                    connexion::depuis_jeton(db, &jeton).await,
                    Err(Erreur::NonAuthentifie)
                ) {
                    let _ = socket.close(4401, "session expirée").await;
                    break;
                }
            }
        }
    }

    hub.diffuse(&Evenement::LiveGone { id });
}

async fn message_client(hub: &Hub, db: &'static Db, id: u64, uid: i32, texte: &str) {
    let Ok(msg) = serde_json::from_str::<MessageClient>(texte) else {
        return;
    };
    match msg {
        MessageClient::Live {
            lat,
            lon,
            label,
            avatar,
        } => {
            if coord_valide(lat, lon) {
                hub.diffuse(&Evenement::Live {
                    id,
                    lat,
                    lon,
                    label: nettoie_label(&label),
                    avatar: nettoie_label(&avatar),
                });
            }
        }
        MessageClient::Alert {
            category,
            lat,
            lon,
            label,
        } => {
            if coord_valide(lat, lon)
                && let Ok(Some(alerte)) = alertes::cree(db, uid, &category, lat, lon, &label).await
            {
                hub.diffuse(&Evenement::Alert(alerte));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_evenements_gardent_leur_forme() {
        let j = serde_json::to_string(&Evenement::PositionsChanged).unwrap();
        assert_eq!(j, r#"{"kind":"positions_changed"}"#);
        let j = serde_json::to_string(&Evenement::LiveGone { id: 3 }).unwrap();
        assert_eq!(j, r#"{"kind":"live_gone","id":3}"#);
        let a = Alerte {
            id: 1,
            category: "police".into(),
            lat: 1.0,
            lon: 2.0,
            label: String::new(),
            ts: 5000,
            confirms: 0,
            denies: 0,
        };
        let j = serde_json::to_value(Evenement::Alert(a)).unwrap();
        assert_eq!(j["kind"], "alert");
        assert_eq!(j["id"], 1);
        assert_eq!(j["ts"], 5000);
    }

    #[test]
    fn les_identifiants_de_connexion_restent_des_nombres_js() {
        let hub = Hub::new();
        let a = hub.nouvel_id();
        let b = hub.nouvel_id();
        assert_ne!(a, b);
        assert!(a < (1u64 << 53) && b < (1u64 << 53));
    }
}

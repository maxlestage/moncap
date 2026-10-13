//! Le temps réel : le WebSocket `/ws` et la diffusion des événements.
//!
//! Chaque événement part à tous les clients connectés à cette instance, et
//! aux autres instances par le pont Redis s'il est configuré. Les messages
//! sont du JSON étiqueté par `"kind"` (snake_case), comme avant.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use paquets::prelude::*;
use paquets::vitesse::ws::{self, Message};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::alertes::{self, Alerte};
use crate::base::maintenant_ms;
use crate::commun::{coord_valide, nettoie_label};
use crate::connexion;
use crate::erreur::{Erreur, Resultat};

/// Le canal de diffusion de l'instance, rangé dans l'état de l'application.
pub struct Hub {
    tx: broadcast::Sender<String>,
    /// Préfixe des identifiants de connexion propre à l'instance : deux
    /// dynos ne donnent plus le même identifiant à deux clients différents.
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

    /// Diffuse un événement à tous les clients — de cette instance, et des
    /// autres via le pont Redis s'il est actif.
    pub fn diffuse(&self, ev: &Evenement) {
        if let Ok(json) = serde_json::to_string(ev) {
            if let Some(publication) = REDIS_PUB.get() {
                let _ = publication.send(json.clone());
            }
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
// Le pont Redis.
// ---------------------------------------------------------------------------

/// Canal vers la tâche de publication Redis (absent : mono-instance).
static REDIS_PUB: OnceLock<tokio::sync::mpsc::UnboundedSender<String>> = OnceLock::new();

const REDIS_CANAL: &str = "moncap:events";

/// URL Redis : REDIS_TLS_URL, sinon REDIS_URL. Heroku Redis a un certificat
/// auto-signé : `#insecure` coupe la vérification du certificat, pas le TLS.
fn url_redis() -> Option<String> {
    let url = std::env::var("REDIS_TLS_URL")
        .or_else(|_| std::env::var("REDIS_URL"))
        .ok()
        .filter(|u| !u.trim().is_empty())?;
    if url.starts_with("rediss://") && !url.contains('#') {
        Some(format!("{url}#insecure"))
    } else {
        Some(url)
    }
}

/// Démarre le pont Redis pub/sub (si REDIS_URL est défini) : les événements
/// d'ici partent vers les autres instances, ceux des autres sont rediffusés
/// ici. Anti-écho par identifiant d'instance ; reconnexion automatique.
pub fn demarre_pont_redis(hub: &'static Hub) {
    let Some(url) = url_redis() else {
        tracing::info!("Redis non configuré : temps réel mono-instance");
        return;
    };
    let client = match redis::Client::open(url) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Redis ignoré (URL invalide) : {e}");
            return;
        }
    };
    let instance = format!("{}-{}", std::process::id(), maintenant_ms());
    let (pub_tx, mut pub_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    if REDIS_PUB.set(pub_tx).is_err() {
        return;
    }

    let client_pub = client.clone();
    let instance_pub = instance.clone();
    tokio::spawn(async move {
        loop {
            match client_pub.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    while let Some(json) = pub_rx.recv().await {
                        let charge = format!("{instance_pub}|{json}");
                        let envoye: Result<(), _> = redis::cmd("PUBLISH")
                            .arg(REDIS_CANAL)
                            .arg(&charge)
                            .query_async(&mut conn)
                            .await;
                        if envoye.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => tracing::warn!("Redis (publication) indisponible : {e}"),
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    tokio::spawn(async move {
        use futures_util::StreamExt;
        loop {
            match client.get_async_pubsub().await {
                Ok(mut pubsub) => {
                    if pubsub.subscribe(REDIS_CANAL).await.is_ok() {
                        tracing::info!("pont Redis pub/sub actif (multi-instances)");
                        let mut flux = pubsub.on_message();
                        while let Some(msg) = flux.next().await {
                            let Ok(charge) = msg.get_payload::<String>() else {
                                continue;
                            };
                            if let Some((source, json)) = charge.split_once('|')
                                && source != instance
                                && !json.is_empty()
                            {
                                let _ = hub.tx.send(json.to_owned());
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("Redis (abonnement) indisponible : {e}"),
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
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

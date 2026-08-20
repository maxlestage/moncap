import { useState } from "react";
import { getApiBase, login, motDePasseOublie, setApiBase, signup } from "./api";

/** Validation d'e-mail volontairement simple : `local@domaine.tld` —
 * la même que l'app iOS. */
const emailValide = (s: string) => /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(s);

/**
 * Écran de connexion / inscription, aux couleurs de la marque.
 *
 * L'identifiant est l'adresse e-mail — comme dans l'app, et comme le dit la
 * politique de confidentialité : elle ne sert qu'à ça, et à retrouver son
 * mot de passe (« Mot de passe oublié ? » envoie le lien). La réponse du
 * serveur est la même que le compte existe ou non : cet écran ne peut pas
 * servir à deviner qui est inscrit.
 */
export function AuthView({ onAuthed }: { onAuthed: () => void }) {
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [apiBase, setApiBaseState] = useState(getApiBase());
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    const adresse = email.trim();
    if (mode === "signup" && !emailValide(adresse)) {
      setError("Adresse e-mail invalide.");
      setInfo("");
      return;
    }
    if (mode === "signup" && password.length < 6) {
      setError("Mot de passe : 6 caractères minimum.");
      setInfo("");
      return;
    }
    setBusy(true);
    setError("");
    setInfo("");
    try {
      setApiBase(apiBase);
      if (mode === "login") await login(adresse, password);
      else await signup(adresse, password);
      onAuthed();
    } catch (e) {
      setError(e instanceof Error && e.message ? e.message : "Échec de l'authentification");
    } finally {
      setBusy(false);
    }
  };

  const oublie = async () => {
    const adresse = email.trim();
    if (!emailValide(adresse)) {
      setError("Adresse e-mail invalide — tape-la au-dessus d'abord.");
      setInfo("");
      return;
    }
    setBusy(true);
    setError("");
    setInfo("");
    try {
      setApiBase(apiBase);
      await motDePasseOublie(adresse);
      setInfo(
        "Si un compte existe avec cette adresse, un lien vient d'être envoyé. Regarde ta boîte (et les indésirables).",
      );
    } catch (e) {
      setError(e instanceof Error && e.message ? e.message : "Serveur injoignable. Réessaie.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-card">
        <div className="auth-logo" aria-hidden="true">
          🛰️
        </div>
        <h1>MonCap GPS</h1>
        <p className="auth-sub">
          {mode === "login" ? "Content de te revoir." : "Bienvenue à bord."}
        </p>

        <label className="auth-field">
          <span>E-mail</span>
          <input
            type="email"
            placeholder="toi@exemple.fr"
            autoCapitalize="none"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </label>
        <label className="auth-field">
          <span>Mot de passe</span>
          <input
            type="password"
            placeholder="6 caractères minimum"
            autoComplete={mode === "signup" ? "new-password" : "current-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        </label>

        {mode === "signup" && (
          <p className="auth-note">
            Ton e-mail sert d'identifiant — il ne sert qu'à ça, et à retrouver ton mot de passe.
          </p>
        )}

        {error && <div className="auth-error">{error}</div>}
        {info && <div className="auth-info">{info}</div>}

        <button className="auth-principal" onClick={submit} disabled={busy || !email || !password}>
          {busy ? "…" : mode === "login" ? "Se connecter" : "Créer mon compte"}
        </button>

        <div className="auth-liens">
          <button
            className="link"
            onClick={() => {
              setMode(mode === "login" ? "signup" : "login");
              setError("");
              setInfo("");
            }}
          >
            {mode === "login" ? "Pas de compte ? S'inscrire" : "Déjà un compte ? Se connecter"}
          </button>
          {mode === "login" && (
            <button className="link" onClick={oublie} disabled={busy}>
              Mot de passe oublié ?
            </button>
          )}
        </div>

        <details className="auth-adv">
          <summary>Serveur (avancé)</summary>
          <input
            placeholder="URL de l'API (vide = même serveur)"
            value={apiBase}
            onChange={(e) => setApiBaseState(e.target.value)}
          />
        </details>
      </div>
      <p className="auth-pied">
        <a href="/confidentialite">Confidentialité</a> · <a href="/">Le site</a>
      </p>
    </div>
  );
}

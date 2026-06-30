import { useState } from "react";
import { login, setApiBase, getApiBase, signup } from "./api";

/** Écran de connexion / inscription. */
export function AuthView({ onAuthed }: { onAuthed: () => void }) {
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [apiBase, setApiBaseState] = useState(getApiBase());
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    setBusy(true);
    setError("");
    try {
      setApiBase(apiBase);
      if (mode === "login") await login(username, password);
      else await signup(username, password);
      onAuthed();
    } catch (e) {
      setError(e instanceof Error && e.message ? e.message : "Échec de l'authentification");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-card">
        <h1>🛰️ MonCap GPS</h1>
        <p className="auth-sub">{mode === "login" ? "Connexion" : "Créer un compte"}</p>

        <input
          placeholder="Nom d'utilisateur"
          autoCapitalize="none"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        <input
          type="password"
          placeholder="Mot de passe"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />

        {error && <div className="auth-error">{error}</div>}

        <button onClick={submit} disabled={busy || !username || !password}>
          {mode === "login" ? "Se connecter" : "S'inscrire"}
        </button>

        <button
          className="link"
          onClick={() => {
            setMode(mode === "login" ? "signup" : "login");
            setError("");
          }}
        >
          {mode === "login" ? "Pas de compte ? S'inscrire" : "Déjà un compte ? Se connecter"}
        </button>

        <details className="auth-adv">
          <summary>Serveur (avancé)</summary>
          <input
            placeholder="URL de l'API (vide = même serveur)"
            value={apiBase}
            onChange={(e) => setApiBaseState(e.target.value)}
          />
        </details>
      </div>
    </div>
  );
}

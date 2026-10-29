// Mise à jour automatique : au démarrage, l'appli regarde sur GitHub si une version plus récente existe.
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

export async function checkForUpdate(show: (message: string, action: string, run: () => Promise<void>) => void) {
  try {
    // D'abord sans clé (dépôt public). Tant que le dépôt est privé, il faut la clé de lecture
    // seule mise dans l'appli à la fabrication.
    let headers: Record<string, string> = { Accept: "application/octet-stream" };
    let update: Update | null = await check({ headers }).catch(() => null);
    if (!update) {
      const token = await invoke<string | null>("update_token");
      if (!token) return;
      headers = { ...headers, Authorization: `Bearer ${token}` };
      update = await check({ headers });
    }
    if (!update) return;
    const found = update;
    show(t("updateAvailable", { version: found.version }), t("updateNow"), async () => {
      await found.downloadAndInstall(undefined, { headers });
      await relaunch();
    });
  } catch {
    // Pas de connexion ou pas encore de version publiée : on réessaiera au prochain lancement.
  }
}

fn main() {
    // Clé de lecture du dépôt (privé) pour la mise à jour automatique : fournie à la fabrication.
    println!("cargo:rerun-if-env-changed=UPDATER_TOKEN");
    tauri_build::build()
}

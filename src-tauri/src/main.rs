// Pas de console noire derrière la fenêtre sous Windows (en version finale).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    documents_lib::run()
}

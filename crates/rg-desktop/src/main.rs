//! Rustgramm для ПК: тонкая оболочка над общим интерфейсом `rg-ui`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    rg_ui::run_desktop()
}

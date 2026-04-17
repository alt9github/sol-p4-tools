use sol_p4_tools::p4;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            p4::get_p4_stream,
            p4::list_p4_workspaces,
            p4::set_p4_connection,
            p4::clear_p4_connection,
            p4::check_stale_revisions,
            p4::check_concurrent_edits,
            p4::get_p4_pending,
            p4::get_p4_diff,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

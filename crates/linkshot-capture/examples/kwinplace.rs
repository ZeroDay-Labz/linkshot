//! Exercises the KWin scripting helpers: `kwinplace query <prefix>`, `kwinplace move <caption> <x> <y>`,
//! `kwinplace fullscreen <caption> <output>`, `kwinplace activate <caption>`, `kwinplace minimize <caption>`,
//! `kwinplace state <prefix>` (geometry, output, active and minimised flags).
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    use linkshot_capture::kwin;
    match a.get(1).map(|s| s.as_str()) {
        Some("query") => { for (c, o, g) in kwin::window_placements(a.get(2).map(|s| s.as_str()).unwrap_or(""))? { println!("{c}\t{o}\t{g:?}"); } }
        Some("move") => kwin::place_window(&a[2], a[3].parse()?, a[4].parse()?)?,
        Some("fullscreen") => kwin::place_fullscreen(&[(a[2].clone(), a[3].clone())])?,
        Some("activate") => kwin::activate_window(&a[2])?,
        Some("minimize") => kwin::minimize_window(&a[2])?,
        Some("unminimize") => kwin::unminimize_window(&a[2])?,
        Some("close") => kwin::close_window(&a[2])?,
        Some("js") => { let body = std::fs::read_to_string(&a[2])?; println!("{}", kwin::run_probe(&body)?); }
        Some("geometry") => kwin::set_geometry(&a[2], a[3].parse()?, a[4].parse()?, a[5].parse()?, a[6].parse()?)?,
        Some("tame") => kwin::tame_helper_window(&a[2], &a[3])?,
        Some("focusaway") => kwin::focus_away_from(&a[2])?,
        Some("state") => { for w in kwin::window_states(a.get(2).map(|s| s.as_str()).unwrap_or(""))? { println!("{}\t{}\t{:?}\tactive={}\tminimized={}", w.caption, w.output, w.geometry, w.active, w.minimized); } }
        _ => println!("usage: query <prefix> | state <prefix> | move <caption> <x> <y> | fullscreen <caption> <output> | activate <caption> | minimize <caption> | unminimize <caption> | close <caption> | js <file> | geometry <caption> <x> <y> <w> <h> | tame <caption> <refocus> | focusaway <caption>"),
    }
    Ok(())
}

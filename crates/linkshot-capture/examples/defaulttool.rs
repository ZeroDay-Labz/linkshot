//! Inspect or change the system-wide Print Screen binding without the GUI:
//! `defaulttool status | make | undo`. `make` expects LinkShot's desktop file to be installed.
use linkshot_platform::default_tool as dt;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "status".into());
    let id = dt::Identity::detect();
    println!("desktop: {:?}, desktop id: {}, capture command: {:?}", dt::desktop(), id.desktop_id, id.capture_command);
    match cmd.as_str() {
        "status" => {
            println!("Print Screen bound to LinkShot: {}", dt::is_default(&id));
            #[cfg(target_os = "linux")]
            if dt::desktop() == dt::Desktop::Kde {
                match dt::kde::holders_of_print() { Ok(h) => for x in h { println!("  Print is held by {} / {} ({} / {})", x.component, x.action, x.component_friendly, x.action_friendly); }, Err(e) => println!("  query failed: {e:#}") }
            }
        }
        "make" => match dt::make_default(&id) { Ok(r) => for s in r.steps { println!("  {s}"); }, Err(e) => { println!("failed: {e:#}"); std::process::exit(1); } },
        "undo" => match dt::undo(&id) { Ok(r) => for s in r.steps { println!("  {s}"); }, Err(e) => { println!("failed: {e:#}"); std::process::exit(1); } },
        other => { eprintln!("unknown command {other}"); std::process::exit(2); }
    }
}

//! End-to-end scrolling-capture test without the UI: runs a `scrolling::Session` on a screen region
//! (logical desktop coordinates) with Automatic Scrolling driving the window under that region,
//! prints every frame result and saves the stitched image.
//!
//!     cargo run -p linkshot-capture --example scrolltest -- <x> <y> <w> <h> [--manual] [--notches N] [--out FILE]
//!
//! The RemoteDesktop restore token is read from linkshot's settings.json so the portal does not
//! prompt again. With `--manual` nothing is scrolled: scroll the content yourself and press Enter.
use linkshot_capture::scrolling::{Command, Event, ScrollOptions, Session};
use linkshot_core::geometry::Rect;
use std::time::{Duration, Instant};

fn settings_token() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let text = std::fs::read_to_string(std::path::PathBuf::from(home).join(".config/linkshot/settings.json")).ok()?;
    let i = text.find("\"remote_desktop_token\"")?;
    let rest = &text[i + "\"remote_desktop_token\"".len()..];
    let q1 = rest.find('"')? + 1;
    let q2 = rest[q1..].find('"')? + q1;
    Some(rest[q1..q2].to_string()).filter(|t| !t.is_empty())
}

fn main() -> anyhow::Result<()> {
    let _ = linkshot_capture::kwin::ensure_authorization("linkshot scrolltest", "linkshot-scrolltest");
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).with_writer(std::io::stderr).init();
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 { eprintln!("usage: scrolltest <x> <y> <w> <h> [--manual] [--notches N] [--out FILE]"); std::process::exit(2); }
    let region = Rect::from_xywh(a[1].parse()?, a[2].parse()?, a[3].parse()?, a[4].parse()?);
    let manual = a.iter().any(|s| s == "--manual");
    let notches = a.iter().position(|s| s == "--notches").and_then(|i| a.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(3);
    let out = a.iter().position(|s| s == "--out").and_then(|i| a.get(i + 1)).cloned().unwrap_or_else(|| "scrolltest.png".into());
    let opts = ScrollOptions { auto: !manual, scroll_from_top: true, horizontal: false, notches, restore_token: settings_token() };
    let session = Session::start(region, opts, false);
    let t0 = Instant::now();
    if manual {
        std::thread::spawn({ let s = session.cmds_clone(); move || { let mut l = String::new(); let _ = std::io::stdin().read_line(&mut l); s.send(Command::Finish); } });
    }
    loop {
        match session.events.recv_timeout(Duration::from_secs(120)) {
            Ok(Event::Progress { frames, accepted, length, last, misses, .. }) => println!("{:7.2}s frame {frames:4} {last:?} accepted={accepted} length={length} misses={misses}", t0.elapsed().as_secs_f64()),
            Ok(Event::AutoFinished) => { println!("auto-scroll finished"); session.send(Command::Finish); }
            Ok(Event::RestoreToken(_)) => println!("(portal restore token received)"),
            Ok(Event::Error(e)) => println!("error: {e}"),
            Ok(Event::Done(img)) => {
                match img { Some(img) => { img.save(&out)?; println!("saved {out} ({}x{})", img.width(), img.height()); } None => println!("no image") }
                break;
            }
            Err(_) => { println!("timeout"); break; }
        }
    }
    Ok(())
}

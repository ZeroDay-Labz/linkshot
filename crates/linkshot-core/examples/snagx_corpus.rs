//! Round-trips every `.snagx` in a directory and reports what did not survive.
//! Usage: cargo run -p linkshot-core --example snagx_corpus -- <dir> [limit]
use linkshot_core::snagx;
use serde_json::Value;

fn canon(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut b = serde_json::Map::new();
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            for k in keys { b.insert(k.clone(), canon(&m[k])); }
            Value::Object(b)
        }
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Number(n) => {
            // Compare numerically with tolerance by normalising formatting.
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 && f.abs() < 1e15 { Value::from(f as i64) } else { Value::from((f * 1e6).round() / 1e6) }
        }
        Value::String(s) => {
            // "x.000000,y.000000" points: normalise float formatting.
            if let Some((a, b)) = s.split_once(',') {
                if let (Ok(x), Ok(y)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                    if !a.trim().is_empty() && !b.trim().is_empty() && !s.contains(' ') {
                        return Value::String(format!("{:.6},{:.6}", x, y));
                    }
                }
            }
            Value::String(s.clone())
        }
        other => other.clone(),
    }
}

fn diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            for (k, va) in ma {
                match mb.get(k) {
                    Some(vb) => diff(va, vb, &format!("{path}/{k}"), out),
                    None => out.push(format!("missing after roundtrip: {path}/{k}")),
                }
            }
            for k in mb.keys() { if !ma.contains_key(k) { out.push(format!("added: {path}/{k}")); } }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.len() != ab.len() { out.push(format!("array len {path}: {} vs {}", aa.len(), ab.len())); }
            for (i, (x, y)) in aa.iter().zip(ab).enumerate() { diff(x, y, &format!("{path}[{i}]"), out); }
        }
        _ => if a != b { out.push(format!("value {path}: {a} vs {b}")); },
    }
}

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).expect("dir");
    let limit: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut files: Vec<_> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "snagx").unwrap_or(false)).collect();
    files.sort();
    let mut ok = 0; let mut failed = 0; let mut diffs = std::collections::BTreeMap::<String, usize>::new();
    for p in files.iter().take(limit) {
        let bytes = std::fs::read(p)?;
        let doc = match snagx::read_bytes(&bytes) { Ok(d) => d, Err(e) => { failed += 1; eprintln!("READ FAIL {}: {e}", p.display()); continue; } };
        // Original page JSON vs re-serialised.
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes))?;
        let idx: Value = serde_json::from_reader(zip.by_name("index.json")?)?;
        let first = idx["Pages"][0].as_str().unwrap().to_string();
        let orig: Value = serde_json::from_reader(zip.by_name(&first)?)?;
        let ours: Value = serde_json::from_slice(&snagx::page_json(doc.page())?)?;
        let mut d = Vec::new();
        diff(&canon(&orig), &canon(&ours), "", &mut d);
        if d.is_empty() { ok += 1; } else { failed += 1; for line in d { let key = line.replace(|c: char| c.is_ascii_digit(), "#"); let key = key.split(" vs ").next().unwrap_or("").to_string(); *diffs.entry(key).or_default() += 1; } }
        let _ = snagx::read_bytes(&snagx::write_bytes(&doc)?)?;
    }
    println!("roundtrip ok: {ok}, with differences: {failed}");
    for (k, n) in diffs.iter().take(40) { println!("  {n:5}  {k}"); }
    Ok(())
}

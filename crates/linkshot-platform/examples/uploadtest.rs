//! uploadtest nextcloud <server> <user> <password> <file>   |   uploadtest imgur <client-id> <file>
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("nextcloud") => {
            let acc = linkshot_platform::upload::NextcloudAccount { server: a[2].clone(), user: a[3].clone(), folder: "LinkShot/Tests ä".into(), public_link: true };
            let bytes = std::fs::read(&a[5])?;
            let name = std::path::Path::new(&a[5]).file_name().unwrap().to_string_lossy().into_owned();
            let u = linkshot_platform::upload::nextcloud_with_password(&acc, &a[4], &bytes, &name)?;
            println!("{u:?}");
        }
        Some("imgur") => {
            let bytes = std::fs::read(&a[3])?;
            println!("{:?}", linkshot_platform::upload::imgur(&bytes, "test.png", &a[2])?);
        }
        _ => eprintln!("usage: uploadtest nextcloud <server> <user> <password> <file> | imgur <client-id> <file>"),
    }
    Ok(())
}

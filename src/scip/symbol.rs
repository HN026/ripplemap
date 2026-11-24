use anyhow::{Result, ensure};

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Package {
    pub scheme: String,
    pub manager: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
}

/**
 * Parse only the SCIP package header; preserve the complete raw symbol as its
 * identity.
 */
pub fn package(symbol: &str) -> Result<Option<Package>> {
    ensure!(!symbol.contains('\0'), "NUL in SCIP symbol");
    if let Some(id) = symbol.strip_prefix("local ") {
        ensure!(
            !id.is_empty() && !id.contains(char::is_whitespace),
            "invalid local symbol"
        );
        return Ok(None);
    }
    let mut remaining = symbol;
    let mut components = Vec::with_capacity(4);
    for _ in 0..4 {
        let bytes = remaining.as_bytes();
        let mut i = 0;
        loop {
            ensure!(i < bytes.len(), "invalid SCIP symbol header: {symbol:?}");
            if bytes[i] == b' ' {
                if bytes.get(i + 1) == Some(&b' ') {
                    i += 2;
                    continue;
                }
                ensure!(i > 0, "empty SCIP symbol component");
                components.push(remaining[..i].replace("  ", " "));
                remaining = &remaining[i + 1..];
                break;
            }
            i += 1;
        }
    }
    ensure!(!remaining.is_empty(), "missing SCIP symbol descriptor");
    let mut parts = components.into_iter();
    let scheme = parts.next().unwrap();
    ensure!(!scheme.starts_with("local"), "invalid global SCIP scheme");
    let value = |part: String| (part != ".").then_some(part);
    Ok(Some(Package {
        scheme,
        manager: value(parts.next().unwrap()),
        name: value(parts.next().unwrap()),
        version: value(parts.next().unwrap()),
    }))
}

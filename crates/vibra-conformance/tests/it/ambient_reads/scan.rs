//! The scanner of the ambient-read audit: the ambient APIs and where a source
//! names them outside comments and `#[cfg(test)]` modules. It names no crate,
//! so `vibra-interp`'s tests include it with `#[path]` and scan their own
//! sources with it.

/// The ambient APIs: the filesystem, the environment, the clocks, the operating
/// system's random source, and the process's standard streams.
pub(crate) const AMBIENT_PATTERNS: &[&str] = &[
    "std::fs",
    "fs::",
    "File::open",
    "File::create",
    "read_dir(",
    "std::env",
    "env::var",
    "env::args",
    "env::current_dir",
    "SystemTime",
    "Instant::now",
    "std::time::Instant",
    "UNIX_EPOCH",
    "getrandom",
    "RandomState",
    "std::io::stdin",
    "io::stdin(",
    "io::stdout(",
    "io::stderr(",
    "std::process",
    "println!",
    "eprintln!",
    "print!(",
    "eprint!(",
];

/// The attribute that gates host-language test code.
const TEST_GATE: &str = "#[cfg(test)]";

/// Whether `text`, trimmed, opens a module: `mod name`, with or without a
/// visibility.
fn opens_module(text: &str) -> bool {
    let text = text.trim_start();
    let text = ["pub(crate) ", "pub(super) ", "pub "]
        .iter()
        .find_map(|visibility| text.strip_prefix(visibility))
        .unwrap_or(text);
    text.starts_with("mod ")
}

/// The lines of `source`, numbered from one, that are not in a
/// `#[cfg(test)]` module: the attribute, the `mod` it gates, and that
/// module's block, matched brace by brace, are left out. A `#[cfg(test)]`
/// field, function, or other item is kept, and so is every line after the
/// module.
pub(crate) fn outside_test_modules(source: &str) -> Vec<(usize, &str)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(TEST_GATE) else {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        };
        // The gated item: on the attribute's line, or on the next line that
        // is neither an attribute nor a comment.
        let mut item = index;
        if rest.trim().is_empty() {
            item += 1;
            while lines.get(item).is_some_and(|text| {
                let text = text.trim_start();
                text.starts_with("#[") || text.starts_with("//")
            }) {
                item += 1;
            }
        }
        let opens = lines.get(item).is_some_and(|text| {
            let text = if item == index { rest } else { text };
            opens_module(text)
        });
        if !opens {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        }
        // Skip through the line that closes the module's block, or through
        // `mod name;`.
        let mut depth: i64 = 0;
        let mut opened = false;
        let mut end = item;
        while let Some(text) = lines.get(end) {
            let text = if end == index { rest } else { text };
            let code = text.split("//").next().unwrap_or(text);
            for character in code.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            end += 1;
            if (opened && depth <= 0) || (!opened && code.trim_end().ends_with(';')) {
                break;
            }
        }
        index = end;
    }
    kept
}

/// The `patterns` `source` names outside of comments and `#[cfg(test)]`
/// modules, which are host-language test code and run no program.
pub(crate) fn references<'p>(
    source: &str,
    patterns: impl Iterator<Item = &'p &'static str> + Clone,
) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (number, line) in outside_test_modules(source) {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let code = line.split("//").next().unwrap_or(line);
        for pattern in patterns.clone() {
            if code.contains(pattern) {
                found.push((number, *pattern));
            }
        }
    }
    found
}

/// The ambient APIs `source` names outside of comments and `#[cfg(test)]`
/// modules, which are host-language test code and run no program.
pub(crate) fn ambient_references(source: &str) -> Vec<(usize, &'static str)> {
    references(source, AMBIENT_PATTERNS.iter())
}

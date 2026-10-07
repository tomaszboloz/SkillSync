/// Retain provenance when upstream replaces a locally installed manifest.
pub(crate) fn with_source(content: &str, source: &str) -> String {
    let mut lines: Vec<_> = content.lines().map(str::to_owned).collect();
    if let Some(end) = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim() == "---")
        .map(|(i, _)| i)
    {
        for index in (1..end).rev() {
            if lines[index].starts_with("skillsync-source:") {
                lines.remove(index);
            }
        }
        lines.insert(1, format!("skillsync-source: {source}"));
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    result
}

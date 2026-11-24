use anyhow::{Result, bail, ensure};

use super::{Occurrence, TypedRange};

impl Occurrence {
    /**
     * Normalize supported SCIP ranges into validated zero-based exclusive coordinates.
     */
    pub fn position(&self) -> Result<[i32; 4]> {
        let position = match &self.typed_range {
            Some(TypedRange::Single(r)) => [r.line, r.start_character, r.line, r.end_character],
            Some(TypedRange::Multi(r)) => {
                [r.start_line, r.start_character, r.end_line, r.end_character]
            }
            None => match self.range.as_slice() {
                [line, start, end] => [*line, *start, *line, *end],
                [line, start, end_line, end] => [*line, *start, *end_line, *end],
                _ => bail!("unsupported occurrence range: expected 3 or 4 coordinates"),
            },
        };
        ensure!(
            position.iter().all(|n| *n >= 0),
            "negative source coordinate"
        );
        ensure!(
            (position[0], position[1]) <= (position[2], position[3]),
            "source range ends before it starts"
        );
        Ok(position)
    }
}

/**
 * Reject absolute, traversing, and noncanonical SCIP document paths.
 */
pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.contains(['\\', ':', '\0'])
            && !path.split('/').any(|part| matches!(part, "" | "." | "..")),
        "invalid SCIP relative document path: {path:?}"
    );
    Ok(())
}

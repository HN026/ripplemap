use super::{Diagnostic, Report, Status};

impl Report {
    /**
     * Finalize full health outcomes before applying the display-only diagnostic limit.
     */
    pub(super) fn finish(&mut self, limit: u32) {
        self.apply_dependency_findings();
        self.status = self
            .diagnostics
            .iter()
            .chain(self.dependencies.iter().flat_map(|d| &d.diagnostics))
            .map(|d| d.status)
            .max()
            .unwrap_or(Status::Unknown);
        self.limit_diagnostics(limit);
    }

    /**
     * Attribute package-level findings conservatively when installation paths are
     * ambiguous.
     */
    fn apply_dependency_findings(&mut self) {
        let mut installations =
            std::collections::BTreeMap::<_, std::collections::BTreeSet<_>>::new();
        for dependency in &self.dependencies {
            installations
                .entry(dependency.dependency.name.clone())
                .or_default()
                .insert(dependency.dependency.target.clone());
        }
        for dependency in &mut self.dependencies {
            let ambiguous = installations[&dependency.dependency.name].len() > 1;
            if ambiguous
                && self.diagnostics.iter().any(|d| {
                    d.dependency.as_deref() == Some(&dependency.dependency.name)
                        && d.code.starts_with("TS")
                })
            {
                dependency.diagnostics.push(Diagnostic::new("typescript_compiler", "ambiguous_installation", Status::Unknown,
                    "Compiler finding names this package, but multiple installation paths exist; no individual copy is identified"));
            }
            dependency.status = self
                .diagnostics
                .iter()
                .filter(|d| d.dependency.as_deref() == Some(&dependency.dependency.name))
                .map(|d| {
                    if ambiguous && d.code.starts_with("TS") {
                        Status::Unknown
                    } else {
                        d.status
                    }
                })
                .chain(dependency.diagnostics.iter().map(|d| d.status))
                .max()
                .unwrap();
        }
    }

    /**
     * Bound displayed diagnostics while preserving full counts and already computed
     * statuses.
     */
    fn limit_diagnostics(&mut self, limit: u32) {
        self.diagnostics_total = self.diagnostics.len()
            + self
                .dependencies
                .iter()
                .map(|d| d.diagnostics.len())
                .sum::<usize>();
        let mut remaining = limit as usize;
        self.diagnostics.truncate(remaining);
        remaining -= self.diagnostics.len();
        for dependency in &mut self.dependencies {
            dependency.diagnostics.truncate(remaining);
            remaining -= dependency.diagnostics.len();
        }
        self.truncated = self.diagnostics_total > limit as usize;
    }
}

//! SigmaOS-apt-ftparchive — Debian Packages/Release index generator.
//!
//! Inspired by Debian's `apt-ftparchive`: builds the `Packages` and `Release`
//! index files an archive serves to apt. Given package metadata it emits
//! deterministic, sorted deb822 `Packages` stanzas plus a `Release` summary
//! (origin, label, date, MD5/SHA checksums of the index files).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::fmt;

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// A package record was missing a required field.
    #[error("package missing required field: {0}")]
    MissingField(&'static str),
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// Metadata for one `.deb` to appear in the Packages index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deb {
    /// Package name (required).
    pub package: String,
    /// Version string (required).
    pub version: String,
    /// Architecture (e.g. `amd64`).
    pub architecture: String,
    /// Section (e.g. `utils`, `libs`).
    pub section: String,
    /// Maintainer field.
    pub maintainer: String,
    /// Short description.
    pub description: String,
    /// Installed-Size in KiB.
    pub installed_size: u64,
    /// Filename relative to the archive root.
    pub filename: String,
    /// SHA256 hex digest of the .deb.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
}

impl Deb {
    /// Validate required fields are present.
    pub fn validate(&self) -> ComponentResult<()> {
        if self.package.is_empty() { return Err(ComponentError::MissingField("Package")); }
        if self.version.is_empty() { return Err(ComponentError::MissingField("Version")); }
        if self.filename.is_empty() { return Err(ComponentError::MissingField("Filename")); }
        Ok(())
    }

    /// Render this record as a deb822 `Packages` stanza.
    pub fn stanza(&self) -> String {
        let mut s = String::new();
        let mut push = |k: &str, v: &str| {
            if !v.is_empty() {
                s.push_str(k);
                s.push_str(": ");
                s.push_str(v);
                s.push('\n');
            }
        };
        push("Package", &self.package);
        push("Version", &self.version);
        push("Architecture", &self.architecture);
        push("Maintainer", &self.maintainer);
        push("Installed-Size", &self.installed_size.to_string());
        push("Filename", &self.filename);
        push("Size", &self.size.to_string());
        push("SHA256", &self.sha256);
        push("Section", &self.section);
        push("Description", &self.description);
        s
    }
}

/// A `Packages` index: sorted set of package records.
#[derive(Debug, Default)]
pub struct PackagesIndex {
    debs: Vec<Deb>,
}

impl PackagesIndex {
    /// Empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a package record.
    pub fn add(&mut self, deb: Deb) -> ComponentResult<()> {
        deb.validate()?;
        self.debs.push(deb);
        Ok(())
    }

    /// Add many records.
    pub fn add_all<I: IntoIterator<Item = Deb>>(&mut self, debs: I) -> ComponentResult<()> {
        for d in debs { self.add(d)?; }
        Ok(())
    }

    /// Render the full `Packages` file, sorted by (Package, Version).
    pub fn render(&self) -> String {
        let mut sorted: Vec<&Deb> = self.debs.iter().collect();
        sorted.sort_by(|a, b| {
            a.package.cmp(&b.package).then_with(|| a.version.cmp(&b.version))
        });
        let mut out = String::new();
        for (i, d) in sorted.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            out.push_str(&d.stanza());
        }
        out
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.debs.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.debs.is_empty()
    }
}

/// Fields for a `Release` file.
#[derive(Debug, Clone, Default)]
pub struct Release {
    /// Origin (e.g. `Debian`).
    pub origin: String,
    /// Label (e.g. `Debian 12.5`).
    pub label: String,
    /// Suite (e.g. `stable`, `testing`).
    pub suite: String,
    /// Codename.
    pub codename: String,
    /// Date (RFC-2822).
    pub date: String,
    /// Architectures covered.
    pub architectures: Vec<String>,
    /// Components (e.g. `main`, `contrib`, `non-free`).
    pub components: Vec<String>,
    /// Index file checksums: filename → (hash algo, hex digest, size).
    pub checksums: BTreeMap<String, (String, String, u64)>,
}

impl Release {
    /// Render the `Release` file.
    pub fn render(&self) -> String {
        let mut s = String::new();
        let mut push = |k: &str, v: &str| {
            if !v.is_empty() {
                s.push_str(k); s.push_str(": "); s.push_str(v); s.push('\n');
            }
        };
        push("Origin", &self.origin);
        push("Label", &self.label);
        push("Suite", &self.suite);
        push("Codename", &self.codename);
        push("Date", &self.date);
        if !self.architectures.is_empty() {
            s.push_str("Architectures:");
            for a in &self.architectures { s.push(' '); s.push_str(a); }
            s.push('\n');
        }
        if !self.components.is_empty() {
            s.push_str("Components:");
            for c in &self.components { s.push(' '); s.push_str(c); }
            s.push('\n');
        }
        if !self.checksums.is_empty() {
            // Group checksums by algorithm.
            let mut by_algo: BTreeMap<String, Vec<(&String, &String, u64)>> = BTreeMap::new();
            for (fname, (algo, hex, size)) in &self.checksums {
                by_algo.entry(algo.clone()).or_default().push((fname, hex, *size));
            }
            for (algo, entries) in by_algo {
                s.push_str(&format!("{}:\n", algo.to_uppercase()));
                for (fname, hex, size) in entries {
                    s.push_str(&format!(" {:64} {:16x} {}\n", fname, size, hex));
                }
            }
        }
        s
    }
}

impl fmt::Display for Release {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deb(name: &str, ver: &str) -> Deb {
        Deb {
            package: name.into(), version: ver.into(),
            architecture: "amd64".into(), maintainer: "A <a@b.c>".into(),
            description: format!("{} package", name), installed_size: 100,
            filename: format!("pool/main/{}/{}-{}.deb", &name[..1], name, ver),
            sha256: "ab".repeat(32), size: 12345,
            section: "utils".into(),
        }
    }

    #[test]
    fn stanza_has_required_fields() {
        let d = deb("bash", "5.2-1");
        let st = d.stanza();
        assert!(st.contains("Package: bash"));
        assert!(st.contains("Version: 5.2-1"));
        assert!(st.contains("Filename:"));
        assert!(st.contains("SHA256:"));
    }

    #[test]
    fn index_sorts_by_name_version() {
        let mut idx = PackagesIndex::new();
        idx.add(deb("zlib", "1.0")).unwrap();
        idx.add(deb("apt", "2.0")).unwrap();
        idx.add(deb("apt", "2.1")).unwrap();
        let out = idx.render();
        let pkgs: Vec<&str> = out.lines()
            .filter(|l| l.starts_with("Package:"))
            .map(|l| l[9..].trim()).collect();
        assert_eq!(pkgs, vec!["apt", "apt", "zlib"]);
    }

    #[test]
    fn missing_required_field_errors() {
        let mut d = deb("x", "1.0");
        d.package = String::new();
        assert!(PackagesIndex::new().add(d).is_err());
    }

    #[test]
    fn release_renders_fields() {
        let mut r = Release::default();
        r.origin = "SigmaOS".into();
        r.label = "SigmaOS 1.0".into();
        r.suite = "stable".into();
        r.architectures = vec!["amd64".into(), "arm64".into()];
        r.components = vec!["main".into()];
        r.checksums.insert("Packages.gz".into(), ("sha256".into(), "cd".repeat(32), 999));
        let out = r.render();
        assert!(out.contains("Origin: SigmaOS"));
        assert!(out.contains("Architectures: amd64 arm64"));
        assert!(out.contains("SHA256:"));
        assert!(out.contains("Packages.gz"));
    }
}

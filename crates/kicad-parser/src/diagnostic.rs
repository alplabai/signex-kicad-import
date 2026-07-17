use std::fmt;

/// Non-fatal fidelity issue discovered while importing a KiCad document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub documentation: Option<&'static str>,
}

impl ImportDiagnostic {
    pub fn unsupported(
        code: &'static str,
        construct: &str,
        count: usize,
        documentation: &'static str,
    ) -> Self {
        Self {
            code,
            message: format!(
                "{count} `{construct}` construct(s) are not represented and were omitted"
            ),
            documentation: Some(documentation),
        }
    }
}

impl fmt::Display for ImportDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "warning[{}]: {}", self.code, self.message)?;
        if let Some(documentation) = self.documentation {
            write!(f, " ({documentation})")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ParseReport<T> {
    pub value: T,
    pub diagnostics: Vec<ImportDiagnostic>,
}

impl<T> ParseReport<T> {
    pub fn new(value: T, diagnostics: Vec<ImportDiagnostic>) -> Self {
        Self { value, diagnostics }
    }
}

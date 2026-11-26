// Draft-only syntactic migration-ledger name parser.
//
// This type validates only the existing shell script's identifier grammar.
// It is not a physical database capability: catalog existence, ownership,
// lineage, transaction binding, and migration authority remain separate
// guards at the caller boundary.

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MigrationLedgerName(String);

impl MigrationLedgerName {
    pub(super) fn parse(value: &str) -> Result<Self, &'static str> {
        if value.is_empty() {
            return Err("migration ledger identifier is required");
        }
        if value.len() > 63 {
            return Err("migration ledger identifier exceeds 63 bytes");
        }
        let bytes = value.as_bytes();
        if !matches!(bytes.first(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_')) {
            return Err("migration ledger identifier has invalid first character");
        }
        if !bytes
            .iter()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'))
        {
            return Err("migration ledger identifier has invalid character");
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn qualified_public_identifier(&self) -> String {
        format!("\"public\".\"{}\"", self.0)
    }
}

#[cfg(test)]
mod phase657_migration_ledger_name_tests {
    use super::*;

    #[test]
    fn parses_shell_compatible_identifier_and_ascii_lowercases_it() {
        let name = MigrationLedgerName::parse("BlueY_Migration_42").unwrap();
        assert_eq!(name.as_str(), "bluey_migration_42");
        assert_eq!(
            name.qualified_public_identifier(),
            "\"public\".\"bluey_migration_42\""
        );
        assert!(name
            .as_str()
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_')));
    }

    #[test]
    fn quotes_reserved_words_as_a_public_identifier() {
        let name = MigrationLedgerName::parse("SELECT").unwrap();
        assert_eq!(name.as_str(), "select");
        assert_eq!(name.qualified_public_identifier(), "\"public\".\"select\"");
    }

    #[test]
    fn accepts_exactly_sixty_three_ascii_bytes() {
        let accepted = format!("a{}", "b".repeat(62));
        assert_eq!(
            MigrationLedgerName::parse(&accepted).unwrap().as_str(),
            accepted
        );
        let rejected = format!("a{}", "b".repeat(63));
        assert_eq!(
            MigrationLedgerName::parse(&rejected),
            Err("migration ledger identifier exceeds 63 bytes")
        );
    }

    #[test]
    fn rejects_invalid_identifier_classes_with_static_errors() {
        for value in [
            "",
            "1ledger",
            "-ledger",
            ".ledger",
            "ledger.name",
            "ledger-name",
            "ledger name",
            "ledger\tname",
            "ledger\nname",
            "ledger\0name",
            "ledger\"name",
            "ledger'name",
            "ledger;name",
            "ledger/name",
            "lédger",
            "表",
        ] {
            let error = MigrationLedgerName::parse(value).expect_err("invalid name must refuse");
            assert!(
                matches!(
                    error,
                    "migration ledger identifier is required"
                        | "migration ledger identifier has invalid first character"
                        | "migration ledger identifier has invalid character"
                ),
                "errors are closed static categories"
            );
        }
        let marker = "phase657_sensitive_rejected_identifier";
        let error = MigrationLedgerName::parse(&format!("{marker}-bad"))
            .expect_err("distinct sensitive marker must refuse");
        assert!(!error.contains(marker), "error must not echo caller input");
    }
}

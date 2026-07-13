use serde::Serialize;

pub const BLUEY_POLICY_SCHEMA_VERSION: u8 = 1;
pub const BLUEY_LICENSE_ID: &str = "LicenseRef-Bluey-Proprietary";
pub const BLUEY_TERMS_URL: &str = "https://bluey.sh/terms";
pub const BLUEY_BUILD_ID: &str = match option_env!("BLUEY_BUILD_ID") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};

// This record is intentionally retained as plain, machine-readable data in
// every binary that links cue-core. It is a policy notice and provenance
// marker, not a claim that client-side metadata can prevent inspection.
#[used]
#[cfg_attr(windows, unsafe(link_section = ".rdata$BLUEY_POLICY"))]
static BLUEY_EMBEDDED_POLICY: [u8; 253] =
    *b"BLUEY_POLICY_JSON_V1\0{\"schema_version\":1,\"product\":\"Bluey\",\"license\":\"LicenseRef-Bluey-Proprietary\",\"automated_extraction\":\"prohibited\",\"model_training\":\"prohibited\",\"redistribution\":\"prohibited\",\"terms_url\":\"https://bluey.sh/terms\",\"notice_only\":true}\0";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EmbeddedProductPolicy {
    pub schema_version: u8,
    pub product: &'static str,
    pub license: &'static str,
    pub build_id: &'static str,
    pub automated_extraction: &'static str,
    pub model_training: &'static str,
    pub redistribution: &'static str,
    pub terms_url: &'static str,
    pub notice_only: bool,
}

pub fn embedded_product_policy() -> EmbeddedProductPolicy {
    std::hint::black_box(&BLUEY_EMBEDDED_POLICY);
    EmbeddedProductPolicy {
        schema_version: BLUEY_POLICY_SCHEMA_VERSION,
        product: "Bluey",
        license: BLUEY_LICENSE_ID,
        build_id: BLUEY_BUILD_ID,
        automated_extraction: "prohibited",
        model_training: "prohibited",
        redistribution: "prohibited",
        terms_url: BLUEY_TERMS_URL,
        notice_only: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_is_machine_readable_and_explicitly_signaling_only() {
        let policy = embedded_product_policy();
        let encoded = serde_json::to_string(&policy).expect("serialize policy");
        assert!(encoded.contains("LicenseRef-Bluey-Proprietary"));
        assert!(encoded.contains("model_training"));
        assert!(encoded.contains("automated_extraction"));
        assert!(policy.notice_only);
        assert!(!policy.build_id.is_empty());
    }
}

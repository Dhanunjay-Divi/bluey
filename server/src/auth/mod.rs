//! JWT / refresh token helpers. Stubs for now; full impl in next commit.

pub mod jwt {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Claims {
        pub sub: String,        // account_id
        pub iat: i64,
        pub exp: i64,
        pub kind: String,       // "access" | "refresh"
    }
}

use crate::db::accounts::Account;

pub(crate) const INTERNAL_TEST_BILLING_BLOCK_MESSAGE: &str =
    "Temporary/internal/test accounts cannot start paid checkout, save payment methods, or enable Auto Reload. Create a regular account before adding credits.";

pub(crate) fn is_internal_or_test_billing_account(account: &Account) -> bool {
    account.is_temporary || account.is_admin || is_internal_or_test_billing_email(&account.email)
}

fn is_internal_or_test_billing_email(email: &str) -> bool {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() {
        return false;
    }

    let bluey_internal = email.ends_with("@bluey.sh")
        && (email.starts_with("internal-")
            || email.starts_with("test-")
            || email.starts_with("admin-test-")
            || email.contains("+test@"));
    bluey_internal || email.ends_with("@test.local") || email.contains("+test@")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_and_test_accounts_cannot_enter_paid_billing_flows() {
        assert!(is_internal_or_test_billing_account(&test_account(
            "internal-admin-20260606023943@bluey.sh",
            false
        )));
        assert!(is_internal_or_test_billing_account(&test_account(
            "owner@bluey.sh",
            true
        )));
        assert!(is_internal_or_test_billing_account(&test_account(
            "person+test@gmail.com",
            false
        )));
        assert!(is_internal_or_test_billing_account(&test_account(
            "dev@test.local",
            false
        )));
        assert!(!is_internal_or_test_billing_account(&test_account(
            "customer@gmail.com",
            false
        )));
    }

    fn test_account(email: &str, is_admin: bool) -> Account {
        Account {
            id: "acct-test".to_string(),
            email: email.to_string(),
            email_verified_at: Some("2026-06-26T00:00:00Z".to_string()),
            balance_cents: 0,
            trial_seconds_remaining: 0,
            is_temporary: false,
            temporary_expires_at: None,
            auto_topup_enabled: false,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 1500,
            is_admin,
            stripe_customer_id: None,
            stripe_payment_method_id: None,
            square_customer_id: None,
            square_card_id: None,
            square_card_brand: None,
            square_card_last4: None,
            billing_restricted: false,
            billing_restriction_reason: None,
            billing_restricted_at: None,
        }
    }
}

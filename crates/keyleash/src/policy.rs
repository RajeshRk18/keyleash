use crate::name::SecretName;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Decision {
    Allow,
    Deny(DenyReason),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    NotAllowed,
}

pub fn decide(name: &SecretName, allowed: &BTreeSet<SecretName>) -> Decision {
    if allowed.contains(name) {
        Decision::Allow
    } else {
        Decision::Deny(DenyReason::NotAllowed)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, DenyReason, decide};
    use crate::name::SecretName;
    use std::collections::BTreeSet;

    #[test]
    fn allows_a_listed_secret() {
        let name = SecretName::try_from("DATABASE_URL").unwrap();
        let allowed = BTreeSet::from([name.clone()]);
        assert_eq!(decide(&name, &allowed), Decision::Allow);
    }

    #[test]
    fn denies_an_unlisted_secret() {
        let name = SecretName::try_from("STRIPE_KEY").unwrap();
        let allowed = BTreeSet::from([SecretName::try_from("DATABASE_URL").unwrap()]);
        assert_eq!(
            decide(&name, &allowed),
            Decision::Deny(DenyReason::NotAllowed)
        );
    }
}
